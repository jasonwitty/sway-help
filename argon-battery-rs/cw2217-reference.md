# CW2217 Fuel Gauge IC Reference

Hardware reference for the Cellwise CW2217 fuel gauge used in the Argon ONE UP
battery board. Gathered from reverse-engineering the Argon daemon
(`argononeupd.py`), Jeff Curless's kernel driver
([JeffCurless/argon-oneup](https://github.com/JeffCurless/argon-oneup)),
and live register reads on our hardware.

---

## I2C

- **Bus**: `/dev/i2c-1` (i2c1)
- **Address**: `0x64`

## Register Map

| Register | Name | R/W | Description |
|----------|------|-----|-------------|
| `0x02`–`0x03` | Voltage | R | Cell voltage, 305 uV/LSB, big-endian unsigned |
| `0x04` | SOC high | R | State of charge, integer percent (0–100) |
| `0x05` | SOC low | R | SOC fractional part, `low / 256.0` percent |
| `0x06`–`0x07` | Temperature | R | Raw temp, unreliable on this board (reads ~0x8200 = garbage, thermistor likely not connected) |
| `0x08` | Control | R/W | `0x00` = active, `0x30` = restart, `0xF0` = sleep |
| `0x0A` | GPIO config | R/W | Interrupt config, write `0x00` to disable |
| `0x0B` | SOC alert | R/W | Bit 7 = battery profile loaded flag |
| `0x0E` | Current high | R | Charge/discharge current, big-endian signed (with `0x0F`) |
| `0x0F` | Current low | R | Current low byte |
| `0x10`–`0x5F` | Profile | R/W | 80-byte OCV curve (battery model), must match chemistry |
| `0xA7` | IC state | R | Bits [3:2] non-zero = IC ready |

## Current Measurement

The CW2217 measures current through an external sense resistor.

- **Sense resistor**: 10 mOhm (on Argon ONE UP board)
- **Raw value**: 16-bit signed big-endian from registers `0x0E:0x0F`
- **Conversion**: `current_mA = 52.4 * raw_signed / (32768 * R_SENSE)`
  - With R_SENSE = 10: `current_mA = 52.4 * raw_signed / 327680`
- **Sign**: Negative = discharging, positive = charging
- **Bit 7 of 0x0E**: Quick charge direction check (1 = discharging, 0 = charging)

### Near-zero current edge case

At high SOC (>95%) on AC power, taper charge current approaches zero. Measurement
noise causes the sign bit to flip, making the IC report "discharging" even though
AC is connected. Specific cases observed:

- **0xFF at 100% SOC**: Charge current is zero, noise produces 0xFF in the high
  byte. Both `argon-battery-rs` and `power-startup` treat `current_high == 0xFF`
  as charging.
- **Other near-zero values** (e.g., 0xEB at 95% SOC = -0.82 mA): Bit 7 is set
  so code reports "discharging" despite AC being present. Our 3-read debounce
  prevents state flickering but the steady-state reading is technically wrong.
  A future improvement could threshold on current magnitude (<2 mA = noise).

## Voltage Reading

- **Registers**: `0x02` (high), `0x03` (low), big-endian unsigned
- **Resolution**: 305 uV per LSB
- **Formula**: `voltage_mV = raw * 305 / 1000`
- **Observed**: 4133 mV at 95% SOC (reasonable for Li-ion)

## Battery Profile (OCV Curve)

The CW2217 needs an 80-byte battery model profile written to registers
`0x10`–`0x5F` that describes the OCV (open circuit voltage) curve for the
specific battery chemistry. Without the correct profile, SOC readings are
inaccurate, especially near full and empty.

### Profile data (from Argon's argononeupd.py)

```
0x32 0x00 0x00 0x00 0x00 0x00 0x00 0x00
0xA8 0xAA 0xBE 0xC6 0xB8 0xAE 0xC2 0x98
0x82 0xFF 0xFF 0xCA 0x98 0x75 0x63 0x55
0x4E 0x4C 0x49 0x98 0x88 0xDC 0x34 0xDB
0xD3 0xD4 0xD3 0xD0 0xCE 0xCB 0xBB 0xE7
0xA2 0xC2 0xC4 0xAE 0x96 0x89 0x80 0x74
0x67 0x63 0x71 0x8E 0x9F 0x85 0x6F 0x3B
0x20 0x00 0xAB 0x10 0xFF 0xB0 0x73 0x00
0x00 0x00 0x64 0x08 0xD3 0x77 0x00 0x00
0x00 0x00 0x00 0x00 0x00 0x00 0x00 0xFA
```

### Profile programming sequence

1. Read `REG_CONTROL` (0x08) — if `!= 0`, IC is not active
2. Read `REG_SOCALERT` (0x0B) — if bit 7 set, profile flag is marked
3. Read and compare all 80 bytes at `0x10`–`0x5F` against known-good profile
4. If mismatch or IC inactive:
   - Write `0x30` to `REG_CONTROL` (restart), wait 500ms
   - Write `0xF0` to `REG_CONTROL` (sleep), wait 500ms
   - Write all 80 profile bytes to `0x10`–`0x5F`
   - Write `0x80` to `REG_SOCALERT` (set profile flag), wait 500ms
   - Write `0x00` to `REG_GPIOCONFIG` (disable interrupts), wait 500ms
   - Restart IC and poll `REG_ICSTATE` for ready

### Profile persistence

The profile survives power cycles — it's stored in the CW2217's non-volatile
memory. On our system, Argon's `argononeupd.py` wrote it during initial setup.
We verified it's still intact (byte-for-byte match, 2026-04-14). If a clean OS
install is done without running the Argon service, the profile would need to be
reprogrammed before SOC readings are accurate.

## Battery Specifications

- **Capacity**: 4800 mAh (per Jeff Curless's driver)
- **Chemistry**: Li-ion
- **Nominal voltage**: 3.7V (estimated 17.76 Wh)
- **Estimated runtime**: ~6 hours (varies with load)
- **Charge time**: ~2.5 hours to full

## Power Baselines (from lid-power-test measurements)

- **Idle floor**: 2.2W (BCM2712 SoC)
- **Total system idle**: ~3.3W
- **Fixed overhead**: ~1W (battery IC, DC-DC converters, Argon MCU)
- **Drain rate**: ~6.4%/hr on battery, ~15-16 hours from full

## References

- [JeffCurless/argon-oneup](https://github.com/JeffCurless/argon-oneup) — Linux kernel driver (GPL-2.0)
- Argon's `argononeupd.py` — original Python daemon (no license, not bundled)
- CW2217 datasheet — not publicly available from Cellwise; register map above
  is reverse-engineered from the above sources
