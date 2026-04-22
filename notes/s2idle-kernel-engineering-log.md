# s2idle / Suspend Engineering Log — Raspberry Pi 5 (BCM2712)

## Goal

Achieve working suspend (s2idle or S2RAM) on the Raspberry Pi 5 to break through
the 2.2W idle floor and dramatically extend battery life on the Argon ONE UP case.

This would be the first known working suspend on a Pi 5.

---

## Background

### Why suspend matters

Lid-close power optimizations (services, USB suspend, ASPM, NVMe power states)
were tested extensively and yielded no improvement over baseline:

| Test | Drain Rate | Notes |
|------|-----------|-------|
| Baseline (lid closed, no optimizations) | ~6.4 %/hr | ~15-16 hrs from 100% |
| Optimized v1 (services + USB + ASPM + laptop_mode) | ~6.4 %/hr | BCM2712 clock gating already handles idle |
| Optimized v2 (+ NVMe PS4 + display off) | ~6.9 %/hr | Worse — measurement noise or NVMe wakeup overhead |

**Key finding**: 2.2W is the hard floor for a running Pi 5. Total system draws ~3.3W
at idle. The ~1W gap is fixed overhead (battery IC, DC-DC converters, Argon board MCU).
No software optimization on a running system can go lower. Suspend is the only path.

### Why Pi 5 has no suspend out of the box

The stock Raspberry Pi OS kernel ships with:
- `CONFIG_SUSPEND` — **disabled**
- `CONFIG_ARM_PSCI_CPUIDLE` — **disabled**

This means:
- `/sys/power/state` only shows `freeze` (a no-op without cpuidle)
- No cpuidle driver registers — CPUs only do basic WFI
- No sleep states of any kind are available

However, the Pi 5 firmware **does** support PSCI v1.1, including CPU_SUSPEND and
SYSTEM_SUSPEND. The hardware capability exists — it just needs kernel-side enablement.

---

## Phase 1: Custom Kernel Build (2026-04-11)

### What we did

Built a custom kernel from the Debian `linux-source-6.12` package to enable suspend.

**Source**: `/usr/src/linux-source-6.12/`

**Config changes** (manually set):
- `CONFIG_SUSPEND=y`
- `CONFIG_ARM_PSCI_CPUIDLE=y`

**Auto-enabled by `olddefconfig`**:
- `CONFIG_PM_SLEEP=y`
- `CONFIG_PM_SLEEP_SMP=y`
- `CONFIG_PM_SLEEP_DEBUG=y`
- `CONFIG_FREEZER=y`
- `CONFIG_SUSPEND_FREEZER=y`
- `CONFIG_ARM_PSCI_CPUIDLE_DOMAIN=y`

**Version string**: `6.12.75-v8-16k-s2idle`

### Build artifacts

| Item | Location |
|------|----------|
| Kernel image | `/boot/firmware/kernel_2712.img` |
| Backup of stock kernel | `/boot/firmware/kernel_2712.img.bak` |
| Modules | `/lib/modules/6.12.75-v8-16k-s2idle/` |
| DTBs | `/boot/firmware/` (updated in place) |
| Build config | `/usr/src/linux-source-6.12/.config` |

### Recovery

If the kernel doesn't boot, mount the SD card / NVMe from another machine and:
```bash
cp /boot/firmware/kernel_2712.img.bak /boot/firmware/kernel_2712.img
```

### Test results — FAILED (hard lock)

**Attempt 1**: `echo freeze > /sys/power/state`
- System became unresponsive
- Display stayed on (backlight + last frame)
- No keypress woke the system
- Had to hold power button to force shutdown

**Attempt 2**: Enabled USB hub and USB driver wake sources, then retried
- Same result — hard lock with display on
- No wake from any input
- Forced power-off again

---

## Phase 1 Post-Mortem: Root Cause Analysis

### PSCI firmware capabilities

From `/sys/kernel/debug/psci` on the running custom kernel:

```
PSCIv1.1
SMC Calling Convention v1.2
OSI is not supported
Original StateID format is used
SYSTEM_SUSPEND is supported
SYSTEM_RESET2 is supported
```

Key takeaways:
- **SYSTEM_SUSPEND is supported** — firmware can do full system-level suspend
- **Original StateID format** — not the extended format from PSCI 1.0
- **OSI not supported** — no OS-initiated hierarchical power domain topology
- **CPU_SUSPEND** — supported (kernel set `psci_cpu_suspend_feature`)

### Sleep states exposed by kernel

```
/sys/power/state = freeze mem
/sys/power/mem_sleep = s2idle [deep]
```

- `freeze` → s2idle path (freeze tasks + cpuidle)
- `mem` → S2RAM path, defaults to `deep` (PSCI SYSTEM_SUSPEND)

### The actual bug: missing DT idle-states

**cpuidle driver = `none`**. The directory `/sys/devices/system/cpu/cpu0/cpuidle/`
does not exist.

The PSCI cpuidle driver (`drivers/cpuidle/cpuidle-psci.c`) initialization:

1. Checks each CPU's `enable-method` = `"psci"` ✅
2. Sets up WFI as state index 0 ✅
3. Calls `dt_init_idle_driver()` to parse DT idle states
4. **Returns `-ENODEV` because there are no `idle-states` in the DTB** ❌
5. Driver registration **aborted** — falls back to `none`

The relevant code path (line 376-378 of `cpuidle-psci.c`):
```c
ret = dt_init_idle_driver(drv, psci_idle_state_match, 1);
if (ret <= 0)
    return ret ? : -ENODEV;  // ← returns -ENODEV when ret == 0
```

The BCM2712 DTB defines CPUs with `enable-method = "psci"` but has **no
`idle-states` node** under the `cpus` node. No other ARM64 platform ships
this way — they all define at least one idle state.

### What happened during freeze

Without a cpuidle driver:
1. `echo freeze > /sys/power/state` → PM core freezes all userspace tasks
2. PM core tries to put CPUs into their deepest idle state
3. With no cpuidle states registered, CPUs fall into a basic WFI loop
4. The WFI has no proper wake event routing configured
5. Interrupts from USB/keyboard never reach the CPU to break WFI
6. System hangs — display pipeline stays powered (never told to stop),
   CPUs stuck in WFI with no exit path

### suspend_stats

```
success: 0
fail: 0
```

Zero successes AND zero failures — the PM framework hung before it could
record an outcome. This confirms the hang happens during the idle entry
phase, not during device suspend.

---

## Phase 2: DT Overlay for Idle States (2026-04-11)

### What we built

A device tree overlay (`psci-idle-states.dtbo`) that adds:

1. An `idle-states` node under `/cpus` with `entry-method = "psci"`
2. A single `cpu-retention` state with conservative parameters
3. `cpu-idle-states` phandle on each of the 4 CPU nodes

### The idle state

```
arm,psci-suspend-param = <0x00000000>
```

In original PSCI StateID format:
- Bit 16 (type) = 0 → **standby/retention** (NOT power-down)
- Bits 25:24 (affinity level) = 0 → CPU-level
- Bits 15:0 (state ID) = 0 → platform-defined minimal idle

This is the lightest possible PSCI idle state. It asks the firmware to put
the core into retention (clock-gated, context preserved) rather than full
power-down. If the firmware doesn't support this exact state, PSCI returns
`DENIED` or `NOT_SUPPORTED` and the kernel falls back to WFI — no hang.

### Timing parameters

```
entry-latency-us = <20>
exit-latency-us = <40>
min-residency-us = <100>
```

Conservative estimates. These tell the cpuidle governor when it's worth
entering this state vs staying in WFI. Can be tuned later with measurements.

### Reference: RK3588 (also Cortex-A76)

The Rockchip RK3588 SoC also uses Cortex-A76 cores with PSCI and defines:

```dts
idle-states {
    entry-method = "psci";
    CPU_SLEEP: cpu-sleep {
        compatible = "arm,idle-state";
        local-timer-stop;
        arm,psci-suspend-param = <0x0010000>;
        entry-latency-us = <100>;
        exit-latency-us = <120>;
        min-residency-us = <1000>;
    };
};
```

Their `0x0010000` = power-down type (bit 16 = 1). We're starting more
conservatively with `0x0000000` (retention/standby) first.

### Installation

| Item | Location |
|------|----------|
| Overlay source | `~/psci-idle-states.dts` |
| Compiled overlay | `/boot/firmware/overlays/psci-idle-states.dtbo` |
| config.txt entry | `dtoverlay=psci-idle-states` (under `[all]`) |

### After reboot — verification ✅

```
cpuidle driver: psci_idle (was: none)
idle states: state0 (WFI), state1 (cpu-retention)
dmesg: cpuidle: using governor menu
```

The overlay loaded correctly and the PSCI cpuidle driver registered.

### Test result — FAILED (watchdog reboot)

**Attempt 3**: `echo freeze > /sys/power/state` (with retention overlay)
- System became unresponsive for ~10 seconds
- No response to initial keypresses
- After aggressive keypressing, screen turned off
- System rebooted on its own (no manual power button needed)

**Different from Phase 1**: display actually turned off, system self-rebooted
instead of hanging forever.

### Phase 2 Post-Mortem: Firmware rejects retention

**Smoking gun**: cpuidle state1 rejection counts after ~4 minutes uptime:

| CPU | Rejections | Usage |
|-----|-----------|-------|
| CPU0 | 126M | 0 |
| CPU1 | 134M | 0 |
| CPU2 | 136M | 0 |
| CPU3 | 132M | 0 |

**~530 million rejections, zero successes.** The BCM2712 firmware does NOT
support PSCI CPU_SUSPEND with param `0x00000000` (standby/retention). Every
call to CPU_SUSPEND returns an error code. The kernel handles this gracefully
during normal operation by falling back to WFI, but the cpuidle governor
wastes cycles on millions of failed attempts per second.

**The self-reboot was the hardware watchdog.** `dmesg` shows:
```
bcm2835-wdt bcm2835-wdt: Broadcom BCM2835 watchdog timer
systemd: Using hardware watchdog, timeout 1min
```

During freeze, systemd is frozen and can't pet the watchdog. After ~1 minute,
the hardware watchdog resets the system. This explains the different behavior
from Phase 1 — the watchdog was always there, but without a cpuidle driver in
Phase 1, the system hung in a way that the freeze path never fully committed
(PM core may have stalled before tasks were fully frozen).

**Persistent journal also failed to survive** — `/var/log/journal/` existed
but was empty. Root cause: needed machine-ID subdirectory. Fixed.

---

## Phase 3: Power-Down State (2026-04-11)

### Rationale

Retention (`0x00000000`) was rejected ~530M times. Other Cortex-A76 PSCI
platforms (notably RK3588) only define power-down states, not retention.
The BCM2712 firmware likely only implements the power-down path for
CPU_SUSPEND.

### What changed

Updated `~/psci-idle-states.dts` overlay:

| Parameter | Phase 2 (retention) | Phase 3 (power-down) |
|-----------|---------------------|----------------------|
| `arm,psci-suspend-param` | `<0x00000000>` | `<0x00010000>` |
| Type bit [16] | 0 (standby) | 1 (power-down) |
| `local-timer-stop` | absent | present |
| `entry-latency-us` | 20 | 100 |
| `exit-latency-us` | 40 | 120 |
| `min-residency-us` | 100 | 1000 |
| State name | `cpu-retention` | `cpu-powerdown` |

Timing values match RK3588 (also Cortex-A76 + PSCI). `local-timer-stop`
tells the kernel the arch timer halts during power-down so it must use
broadcast timer infrastructure.

### Installation

Compiled and installed to `/boot/firmware/overlays/psci-idle-states.dtbo`.
`config.txt` unchanged (`dtoverlay=psci-idle-states` still in `[all]`).

### Verification steps after reboot

```bash
# 1. Confirm kernel
uname -r  # Expected: 6.12.75-v8-16k-s2idle

# 2. Check cpuidle driver
cat /sys/devices/system/cpu/cpuidle/current_driver  # Expected: psci_idle

# 3. Check new state name
cat /sys/devices/system/cpu/cpu0/cpuidle/state1/name  # Expected: cpu-powerdown

# 4. Check rejection count after ~30 seconds
cat /sys/devices/system/cpu/cpu0/cpuidle/state1/rejected
cat /sys/devices/system/cpu/cpu0/cpuidle/state1/usage
# If rejected >> 0 and usage == 0 → firmware rejects this param too
# If usage > 0 → firmware accepts power-down! Proceed to freeze test

# 5. Enable PM debug before freeze test
echo 1 | sudo tee /sys/power/pm_debug_messages

# 6. Test freeze (only if usage > 0)
sudo sh -c 'echo freeze > /sys/power/state'
# Expected: system suspends, wakes on keypress
# If watchdog reboot: check journalctl -b -1 (now persistent)
```

### After reboot — verification ✅

```
uname -r = 6.12.75-v8-16k-s2idle
cpuidle states: state0 (WFI), state1 (cpu-powerdown)
```

**cpuidle results (cpu0 after ~5 min uptime):**

| State | Name | Usage | Rejected |
|-------|------|------:|--------:|
| 0 | WFI | 1,810,607 | 0 |
| 1 | cpu-powerdown | 0 | 38,624,619 |

**Same outcome as retention.** ~38.6M rejections, zero usages. The BCM2712
firmware does NOT implement CPU_SUSPEND with power-down param `0x00010000`
either. This conclusively rules out PSCI CPU_SUSPEND on this platform —
the firmware only advertises the function ID but rejects all state parameters.

### Persistent journal

Still not surviving reboots — `journalctl -b -1` reports no persistent
journal found. The machine-ID subdirectory fix from last session didn't
stick, or journald reverted to volatile storage. Needs another fix.

### Conclusion: CPU_SUSPEND is a dead end on BCM2712

Both retention (`0x00000000`) and power-down (`0x00010000`) are rejected.
The firmware exposes CPU_SUSPEND as a PSCI function but doesn't implement
any actual idle states for it. This is consistent with the Pi Foundation's
decision to ship with `CONFIG_ARM_PSCI_CPUIDLE=n` — they know it doesn't
work.

**Next: S2RAM via SYSTEM_SUSPEND** (Test D). This is a completely different
code path — full system suspend, not per-CPU idle. The PSCI debugfs
explicitly confirms `SYSTEM_SUSPEND is supported`.

---

## Phase 4: S2RAM via SYSTEM_SUSPEND (2026-04-11)

### Rationale

CPU_SUSPEND is conclusively dead on BCM2712 — both retention and power-down
params rejected by firmware. SYSTEM_SUSPEND is a completely different code
path: full system suspend orchestrated by firmware, not per-CPU idle states.
PSCI debugfs explicitly confirms `SYSTEM_SUSPEND is supported`.

### Pre-test verification (session 6, post-reboot)

| Check | Result |
|-------|--------|
| Kernel | `6.12.75-v8-16k-s2idle` ✅ |
| cpuidle driver | `psci_idle` ✅ |
| Persistent journal | Working — `-b -1` shows previous boot ✅ |
| `/sys/power/state` | `freeze mem` ✅ |
| `/sys/power/mem_sleep` | `s2idle [deep]` — defaults to deep (SYSTEM_SUSPEND) ✅ |

### What `echo mem > /sys/power/state` does

1. PM core suspends all devices (USB, PCIe, DRM, etc.)
2. Non-boot CPUs taken offline
3. `psci_system_suspend()` called with resume entry point address
4. Firmware receives PSCI SYSTEM_SUSPEND call
5. Firmware puts system into lowest power state (potentially DRAM self-refresh)
6. Wake event → firmware restores state → jumps to resume entry
7. Boot CPU resumes → other CPUs brought back → devices resumed

### Test procedure

```bash
echo 1 | sudo tee /sys/power/pm_debug_messages
sudo dmesg -C
sudo sh -c 'echo mem > /sys/power/state'
# If wake: dmesg for PM debug trace
# If hang/reboot: journalctl -b -1 for crash logs
```

### Test result — FAILED (firmware bounce + broken resume)

**Boot -2 in journalctl** (Apr 11, 21:25–21:27):

**Suspend sequence** (21:25:28–21:25:35):
1. `PM: suspend entry (deep)` — S2RAM initiated
2. Filesystems synced (0.041s)
3. Userspace frozen (0.001s), kernel tasks frozen (6.252s — slow, but completed)
4. **WiFi (brcmfmac) failed to suspend**: `brcmf_ops_sdio_suspend: Failed to set pm_flags 1`
   - Kernel WARNING at `mmc_sdio_suspend+0x2c/0x140`
   - Bus already down: `brcmf_fil_cmd_data: bus is down. we have nothing to do`
5. Despite warning, PM continued — all devices suspended (603ms)
6. Non-boot CPUs killed via PSCI: CPU3, CPU2, CPU1 (each polled 0ms)
7. Syscore suspend completed: timekeeping → IRQ → KVM → firmware → cpu_pm

**Firmware bounce** (21:25:35):
8. `cpu_pm_suspend` called → `cpu_pm_resume` called immediately after
9. `Timekeeping suspended for 0.049 seconds` — **firmware returned in 49ms**
10. SYSTEM_SUSPEND was accepted (no error) but did NOT enter a low-power state

**Resume catastrophe** (21:25:35 → 21:27:05):
- **V3D GPU hung**: Infinite `Resetting GPU for hang` + `Failed to wait for SMS reset`
  - V3D_ERR_STAT alternated between `0x00001000` (first) and `0x00000000`
  - GPU reset loop at ~1/second, non-stop for 90+ seconds
  - SMS (shader multiprocessor scheduler) wouldn't reset — hardware state corrupted
- **I2C bus dead**: `i2c_designware 1f00074000.i2c: controller timed out` (every ~2s)
  - Battery gauge, DDC brightness — all I2C peripherals unreachable
- **WiFi dead**: `brcmf_sdio_bus_sleep: error -110` + `RXHEADER FAILED: -110` (every ~4s)
  - SDIO interface completely unresponsive, RX frames all failing
- **NVMe**: Resumed OK (`nvme0: 4/0/0 default/read/poll queues`) — only thing that survived
- **PCIe**: Both links re-established (8.0 GT/s x1 and 5.0 GT/s x4)
- **Display**: Black screen — V3D hung means compositor frozen

**User intervention** (21:27:05):
- Power button pressed → `systemd-logind: Power key pressed short` → clean shutdown
- Plymouth poweroff screen appeared (the shutdown text on black screen)
- Clean unmount of filesystems, orderly service teardown

### Analysis

SYSTEM_SUSPEND follows the same pattern as CPU_SUSPEND: the PSCI function
is advertised as supported but the firmware doesn't actually implement the
power-state transition. CPU_SUSPEND rejected calls with error codes;
SYSTEM_SUSPEND is worse — it returns success but does nothing, and the
suspend/resume round-trip corrupts hardware state for devices that lack
proper resume sequences (V3D, I2C, WiFi SDIO).

This conclusively rules out **all PSCI-based sleep paths** on the BCM2712:
- CPU_SUSPEND retention (0x00000000): rejected 530M times
- CPU_SUSPEND power-down (0x00010000): rejected 38.6M times
- SYSTEM_SUSPEND: accepted but no-op, corrupts hardware on resume

---

## Other Infrastructure Changes

### Persistent journal

**Root cause found (session 5)**: Raspberry Pi OS ships
`/usr/lib/systemd/journald.conf.d/40-rpi-volatile-storage.conf` which forces
`Storage=volatile`, overriding any setting in `/etc/systemd/journald.conf`.
Drop-ins in `/usr/lib/` load after the main config file, so the RPi default wins.

**Fix**: Created `/etc/systemd/journald.conf.d/50-persistent.conf` with
`Storage=persistent`. Drop-ins in `/etc/` override `/usr/lib/` at the same
numeric prefix, and `50-` sorts after `40-` regardless.

```bash
sudo mkdir -p /etc/systemd/journald.conf.d
printf '[Journal]\nStorage=persistent\n' | sudo tee /etc/systemd/journald.conf.d/50-persistent.conf
sudo systemctl restart systemd-journald
sudo journalctl --flush
```

Previous attempts (mkdir + tmpfiles + restart) didn't work because the
RPi volatile drop-in kept overriding the config.

---

## Current Theories

### Theory 1: ~~s2idle with cpuidle retention~~ DISPROVED

Firmware rejected retention (param `0x00000000`) 530M times with zero
successes. BCM2712 does not implement retention-level CPU_SUSPEND.

### ~~Theory 1b: s2idle with cpuidle power-down~~ DISPROVED

Power-down param (`0x00010000`) also rejected — 38.6M rejections, 0 usages.
BCM2712 firmware does not implement CPU_SUSPEND for ANY state parameter.
Combined with Theory 1, this rules out the entire cpuidle/s2idle approach.

### ~~Theory 2: S2RAM via SYSTEM_SUSPEND~~ DISPROVED

Firmware accepts the call but returns in 49ms without entering low-power
state. Resume path corrupts V3D, I2C, and WiFi SDIO. Firmware advertises
SYSTEM_SUSPEND but does not implement it.

### ~~Theory 3: Power-down idle state instead of retention~~ DISPROVED

Merged into Theory 1b above — both retention and power-down rejected.

### ~~Theory 4: The display pipeline is the problem~~ MOOT

Irrelevant now that all PSCI suspend paths are dead. Display suspend
would only matter if there were a working sleep state to enter.

---

## Phase 5: Beyond PSCI — Alternative Approaches

All standard Linux PM paths (s2idle, S2RAM) depend on PSCI, which the
BCM2712 firmware stubs out. To break the 2.2W floor, we need to bypass
PSCI entirely or bypass Linux PM entirely.

### Approach A: Clock/power domain gating via firmware mailbox

The Pi 5 firmware has its own mailbox interface (`/dev/vcio`, VideoCore
mailbox) that controls clocks and power domains independently of PSCI.
The stock kernel already uses this for `SET_CLOCK_RATE`, camera/codec
power domains, etc.

**Targets for lid-close gating:**
- **V3D GPU clock** — draws real power even when idle. The kernel has
  `v3d_pm_ops` but they only trigger during system suspend. Could use
  `pm_runtime_force_suspend` or disable the V3D power domain via mailbox.
- **ISP/HEVC/H264 decoder blocks** — may be powered up even if unused.
- **HDMI PHY clocks** — DDC/HDMI output draws power even with display off.

**Approach:** Write a lid-close hook that powers down individual domains
via the firmware mailbox, then restores them on lid-open. System stays
running (userspace alive, WiFi up) but non-essential hardware is gated.

**Risk:** Medium — power domain on/off is what the firmware is designed
for. V3D re-init on restore is the main concern (compositor crash).

**Potential gain:** Unknown but could be meaningful — V3D and HDMI PHY
are non-trivial consumers. Needs measurement.

### Approach B: CPU frequency/voltage floor

BCM2712 supports DVFS. At idle the governor drops frequency, but the
voltage rail may not follow to its minimum. Forcing the absolute minimum
OPP (operating performance point) during lid-close and confirming the
PMIC actually drops Vcore could shave power.

**Investigation:**
```bash
cat /sys/devices/system/cpu/cpufreq/policy0/scaling_available_frequencies
cat /sys/kernel/debug/clk/arm/clk_rate
# Check if voltage actually changes at min freq:
cat /sys/kernel/debug/regulator/*/microvolts  # if exposed
```

**Risk:** Low — this is just setting the governor and max_freq.

**Potential gain:** Probably small (tens of mW). BCM2712 already clock-
gates aggressively at idle, so the CPU power draw at min freq vs idle
WFI may be negligible.

### Approach C: DRAM self-refresh forcing

DRAM is typically the biggest power consumer in a sleeping system. The
BCM2712 memory controller may support manual self-refresh entry without
requiring SYSTEM_SUSPEND — a direct register write to the DRAM controller
MMIO region.

**Investigation:** Find the memory controller base address in the DTB,
identify the self-refresh control register, and test with devmem2.

**Risk:** Very high — incorrect register writes = instant system crash
and potential DRAM corruption. Would need the BCM2712 TRM (technical
reference manual) which Broadcom has not published.

**Potential gain:** Potentially large — DRAM self-refresh could save
hundreds of mW. But this is the thing SYSTEM_SUSPEND was supposed to do.

### Approach D: Argon MCU hibernate (shutdown + scheduled wake)

The Argon ONE UP has its own microcontroller managing battery, power
button, and fan. If the MCU can be commanded to cut power to the Pi
completely and wake on a timer or button press:

1. Save state to NVMe (kernel hibernation image or kexec snapshot)
2. Tell Argon MCU to cut SoC power rail
3. MCU wakes Pi on button press (or optionally on timer)
4. Pi boots → restores hibernation image → back to previous state

**Investigation:**
- Reverse-engineer Argon MCU I2C protocol for power control commands
- Check if MCU supports scheduled wake (timer-based power-on)
- Test kernel hibernation (`echo disk > /sys/power/state`) to NVMe
- Measure MCU-only power draw (with Pi powered off)

**Risk:** Medium-high — hibernation is complex (all device state must
serialize/deserialize correctly), and MCU protocol is undocumented.

**Potential gain:** Highest of all approaches — near-zero SoC power.
MCU + battery IC quiescent draw only. Could extend standby from ~16hrs
to days/weeks. But wake latency would be full boot time (10-20s).

### Approach E: Cortex-A76 implementation-defined idle (bypass PSCI)

The Cortex-A76 has `CPUPWRCTLR_EL1` (CPU Power Control Register) that
can request core retention or power-down directly, bypassing PSCI. Some
vendors use this when their firmware doesn't implement PSCI idle states.

**Investigation:**
```
MRS x0, S3_0_C15_C2_7   ; read CPUPWRCTLR_EL1
; bit 0: CORE_PWRDN_EN — request power-down on WFI
```

A kernel module could set `CORE_PWRDN_EN` on all cores before entering
WFI during s2idle, potentially achieving core power-down without PSCI.

**Risk:** High — the power controller hardware (GIC, interconnect) must
be configured to actually honor the request. Without firmware cooperation,
the core may power down but fail to wake (same hang as Phase 1). Also,
EL3 firmware may trap writes to this register.

**Potential gain:** Moderate — core power savings only (DRAM, I/O, and
peripherals stay fully powered). Might shave 100-300mW from the 2.2W
floor if it works.

### Priority order

1. **Approach A** (clock/power domain gating) — safest, most likely to yield
   measurable results, can be done incrementally
2. **Approach D** (MCU hibernate) — highest potential gain, but requires
   MCU reverse engineering and hibernation work
3. **Approach B** (CPU freq/voltage) — quick to test, probably minimal gain
4. **Approach E** (CPUPWRCTLR_EL1) — interesting but may be trapped by EL3
5. **Approach C** (DRAM self-refresh) — needs BCM2712 TRM, very dangerous

---

## Completed Test Plan

### ~~Test A: Verify overlay loaded correctly~~ ✅ DONE
- Overlay loaded, psci_idle driver registered, cpu-retention state visible

### ~~Test B: s2idle with retention state~~ ✅ DONE — FAILED
- Firmware rejects retention param (530M rejections)
- Watchdog rebooted system after ~1 min

### ~~Test C: s2idle with power-down state~~ ✅ DONE — FAILED
- Overlay loaded, `cpu-powerdown` state registered
- 38.6M rejections, 0 usages — firmware rejects power-down just like retention
- **CPU_SUSPEND is conclusively dead on BCM2712**

### ~~Test D: S2RAM via SYSTEM_SUSPEND~~ ✅ DONE — FAILED
- `echo mem > /sys/power/state` — firmware bounced back in 49ms
- SYSTEM_SUSPEND accepted but not actually implemented — same pattern as CPU_SUSPEND
- Resume left V3D GPU, I2C bus, and WiFi SDIO in broken state
- System appeared hung (black screen, GPU in infinite reset loop)
- Power button triggered clean shutdown via systemd-logind
- **SYSTEM_SUSPEND is also a dead end on BCM2712**

### ~~Test E: Power measurement~~ CANCELLED
- No working sleep state to measure

---

## File Inventory

| File | Location | Purpose |
|------|----------|---------|
| Custom kernel | `/boot/firmware/kernel_2712.img` | 6.12.75-v8-16k-s2idle with SUSPEND + PSCI_CPUIDLE |
| Stock kernel backup | `/boot/firmware/kernel_2712.img.bak` | Recovery — copy over kernel_2712.img |
| Kernel modules | `/lib/modules/6.12.75-v8-16k-s2idle/` | Matching modules for custom kernel |
| Kernel build config | `/usr/src/linux-source-6.12/.config` | Full config used for build |
| DT overlay source | `~/psci-idle-states.dts` | Adds idle-states to BCM2712 DT |
| DT overlay binary | `/boot/firmware/overlays/psci-idle-states.dtbo` | Compiled overlay |
| Boot config | `/boot/firmware/config.txt` | `dtoverlay=psci-idle-states` at end |
| PSCI cpuidle driver | `drivers/cpuidle/cpuidle-psci.c` | Kernel source — reference for behavior |
| DT idle states parser | `drivers/cpuidle/dt_idle_states.c` | Kernel source — reference for DT parsing |
| lid-power-test | `/usr/local/bin/lid-power-test` | Battery drain measurement tool |

---

## Changelog

- **2026-04-11 (session 1)**: Built custom kernel with CONFIG_SUSPEND + CONFIG_ARM_PSCI_CPUIDLE
- **2026-04-11 (session 2)**: First freeze test — hard lock, display stayed on
- **2026-04-11 (session 2)**: Enabled USB wake sources, second freeze test — same hard lock
- **2026-04-11 (session 3)**: Root cause analysis — no idle-states in DT, cpuidle driver = none
- **2026-04-11 (session 3)**: Discovered SYSTEM_SUSPEND is supported via PSCI debugfs
- **2026-04-11 (session 3)**: Built and installed psci-idle-states DT overlay (retention state)
- **2026-04-11 (session 3)**: Enabled persistent journaling for crash log survival
- **2026-04-11 (session 3)**: Reboot pending to test overlay
- **2026-04-11 (session 4)**: Tested freeze with retention overlay — watchdog reboot after ~1 min
- **2026-04-11 (session 4)**: Found 530M cpuidle state1 rejections — firmware rejects retention param
- **2026-04-11 (session 4)**: Fixed persistent journal (needed machine-ID subdirectory)
- **2026-04-11 (session 4)**: Rebuilt overlay with power-down state (0x00010000) + local-timer-stop
- **2026-04-11 (session 5)**: Power-down overlay confirmed loaded — 38.6M rejections, 0 usages
- **2026-04-11 (session 5)**: CPU_SUSPEND conclusively dead on BCM2712 (both params rejected)
- **2026-04-11 (session 5)**: Persistent journal still not surviving reboots — needs another fix
- **2026-04-11 (session 5)**: Fixed persistent journal root cause: RPi `40-rpi-volatile-storage.conf` override
- **2026-04-11 (session 5)**: Moving to Test D — reboot first for clean persistent logs, then S2RAM
- **2026-04-11 (session 6)**: Post-reboot verification — kernel, cpuidle driver, overlay all good
- **2026-04-11 (session 6)**: Persistent journal confirmed working — previous boot (session 5) logs visible
- **2026-04-11 (session 6)**: Attempting Test D — S2RAM via `echo mem > /sys/power/state`
- **2026-04-14 (session 7)**: Analyzed Test D logs — firmware bounced back in 49ms, V3D/I2C/WiFi broken on resume
- **2026-04-14 (session 7)**: All PSCI sleep paths conclusively dead on BCM2712
- **2026-04-14 (session 7)**: Updated log with full Test D results and final conclusions
- **2026-04-14 (session 7)**: Added Phase 5 — five alternative approaches beyond PSCI
