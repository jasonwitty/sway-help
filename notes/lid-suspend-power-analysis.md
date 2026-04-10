# Lid Suspend Power Optimization — Analysis

## Current lid-suspend Actions

1. Lock screen (swaylock)
2. Display off (swaymsg output * power off)
3. CPU governor → powersave
4. WiFi + Bluetooth rfkill blocked
5. Webcam USB unbound

---

## Table 1: Distro Improvements (for all users)

### BCM2712 Power Architecture (important context)

The Pi 5's BCM2712 has effective clock gating and DVFS. Key implications:

- **Clock gating**: idle cores have their clocks stopped. An idle core at 1.5GHz draws the same as one at 600MHz.
- **Voltage floor**: 720mV minimum (SRAM stability), which already supports 1500MHz. Going below 1.5GHz saves zero voltage.
- **Core offlining is negligible**: clock gating already effectively "turns off" idle cores. Forum measurements confirm near-zero difference between 1 and 4 cores at true idle.
- **Frequency caps are pointless at idle**: powersave governor + clock gating already handle this. Capping max freq only prevents bursts, but nothing bursts with lid closed.
- **What actually costs power**: wakeups. Every timer, poll, or I/O wakes the CPU, un-gates the clock, ramps voltage. Reducing wakeups is the real lever.

Sources:
- Jeff Geerling: overclocking/underclocking Pi 5 — idle power identical across clock speeds
- RPi Forums: BCM2712 power gating deep dive — 720mV floor, clock gating effective
- RPi Forums: Pi 5 minimum idle power — core count doesn't affect idle draw

### Hardware Power (add to lid-suspend close/open)

| Action | Close | Open | Impact | Why |
|--------|-------|------|--------|-----|
| PCIe ASPM powersupersave | `echo powersupersave > /sys/module/pcie_aspm/parameters/policy` | `echo default` | High | Lets NVMe link enter deeper sleep between wakeups |
| USB auto-suspend all | `echo auto` on all USB power/control | `echo on` for keyboard/audio | High | Stops USB polling/interrupt overhead on keyboard, audio adapter |
| Laptop mode on | `echo 5 > /proc/sys/vm/laptop_mode` | `echo 0` | Med | Batches disk writes, fewer NVMe wakeups |
| Dirty writeback 60s | `echo 6000 > /proc/sys/vm/dirty_writeback_centisecs` | restore to 500 | Med | Reduces periodic flush wakeups from every 5s to every 60s |
| Fan PWM → 0 | write 0 to hwmon pwm1 | let daemon/kernel take over | Low | Fan motor draws some power; temp drops fast lid-closed |

### Removed from plan (ineffective on BCM2712)

| Action | Why Removed |
|--------|-------------|
| ~~Offline CPU cores 1-3~~ | Clock gating already stops idle cores. Negligible measured difference. |
| ~~Cap CPU freq at 1.5GHz~~ | Powersave governor already drops to 1.5GHz at idle. Clock gating means frequency is irrelevant when idle. No voltage benefit below 1.5GHz (720mV floor). |

### Installer-Provided Services (stop on close, restart on open)

| Service | Installed By | Why Stop |
|---------|-------------|----------|
| pipewire + pipewire-pulse + wireplumber | installer (apt) | No audio with lid closed |
| bluetooth.service | base OS, blueman from installer | Already rfkill blocked, stop daemon too |
| mpris-proxy + obex (user) | blueman dependency | BT media/file transfer, useless |
| xdg-desktop-portal + portal-wlr + portal-gtk (user) | installer (apt) | Screen sharing/portals, no screen |
| at-spi-dbus-bus (user) | GTK dependency chain | Accessibility bus, no input |
| gvfs-* monitors (user) | thunar dependency | 5 filesystem monitors (MTP, AFC, gphoto2, GOA, udisks2) |
| evolution-addressbook-factory + source-registry (user) | GNOME dep chain | PIM services nobody asked for |
| gnome-keyring-daemon (user) | gtkgreet/GTK deps | No auth prompts happening |

### Base OS Services (could disable permanently in installer)

| Service | Why Unnecessary |
|---------|----------------|
| ModemManager | No modem in Argon ONE UP |
| serial-getty@ttyAMA10 | Serial console login, not useful for desktop |
| switcheroo-control | GPU switching proxy, Pi has one GPU |
| avahi-daemon | mDNS, most users don't need LAN discovery |
| wpa_supplicant | NetworkManager handles wifi, wpa_supplicant runs redundantly |

---

## Table 2: Jason's Personal Additions (not from installer)

| Service | What It Is | Suggestion |
|---------|-----------|------------|
| docker + containerd | Container runtime, 0 containers running | `systemctl disable` — enable when needed |
| fwupd | Firmware updater, runs on-demand anyway | `systemctl disable` — run manually |
| smartmontools | Disk health polling | `systemctl disable` — run manually |

---

## Proposed: `~/.config/lid-suspend.d/` Drop-In Support

Allow users to define their own services to stop/start on lid events without editing the core script.

### Structure

```
~/.config/lid-suspend.d/
├── 10-docker.conf
├── 20-socktop.conf
└── README
```

### Format

```ini
# ~/.config/lid-suspend.d/10-docker.conf
# Stop Docker when lid closes to save battery
[close]
systemctl stop docker containerd

[open]
systemctl start docker containerd
```

### Integration

`lid-suspend` scans the `.d` directory after core distro actions:

```bash
CONF_DIR="$HOME/.config/lid-suspend.d"
if [ -d "$CONF_DIR" ]; then
    for conf in "$CONF_DIR"/*.conf; do
        # parse and run the [close] or [open] section
    done
fi
```

### Benefits

- Distro handles hardware power + its own services (Table 1)
- Users drop in files for their own stuff without editing the core script
- Ship example .conf files in repo under `examples/lid-suspend.d/`
- Numbering prefix controls execution order

---

## Open Decisions

1. Which base OS services to disable permanently in installer vs stop/start on lid events
2. Sudoers entries needed for new power actions (ASPM, laptop_mode, dirty_writeback)
3. Whether to tie fan PWM control into lid-suspend directly or let the future argon-fan daemon handle it
4. How aggressive to be with USB suspend (some devices don't wake cleanly from auto-suspend)
5. Whether pipewire socket-activation is enough to restart it on open, or if explicit start is needed
6. Format details for the .d config files (INI sections vs simpler line-based format)
7. Measure actual watt savings with `vcgencmd pmic_read_adc` before/after each change to validate impact
