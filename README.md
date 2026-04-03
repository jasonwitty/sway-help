# sway-argon-one-up

Sway window manager configuration for the [Argon ONE UP CM5 Laptop](https://argon40.com/products/argon-one-up-cm5-laptop-core-system), a 14-inch laptop powered by the Raspberry Pi Compute Module 5. Includes Catppuccin Frappe theming, a dynamic keybinding help overlay, hybrid brightness control, and Claude Code integration.

![screenshot](screenshot.png)

## Hardware

This config is built for the [Argon ONE UP CM5 Laptop](https://argon40.com/products/argon-one-up-cm5-laptop-core-system) which uses a Raspberry Pi Compute Module 5. The display is connected via HDMI internally, so standard backlight controls don't apply — brightness uses a hybrid approach: [wl-gammarelay-rs](https://github.com/MaxVerevkin/wl-gammarelay-rs) provides instant gamma adjustment on every key press, while `ddcutil` sets the real panel backlight in the background. The Argon case also has its own battery, monitored via a custom script.

## What's included

| Directory | Description |
|-----------|-------------|
| `sway/` | Sway config with Catppuccin Frappe window colors, idle lock, touchpad, media keys |
| `waybar/` | Top bar with workspaces, clock, CPU, volume, backlight, Argon battery, tray, Claude + help + power buttons |
| `wob/` | Wayland Overlay Bar config for brightness/volume indicators |
| `wofi/` | App launcher and help overlay styles |
| `foot/` | Terminal emulator with Frappe 16-color palette |
| `mako/` | Notification daemon themed to match |
| `swaylock/` | Lock screen with Frappe colored ring indicator |
| `gtk-3.0/` | GTK dark theme settings |
| `bin/` | `sway-help`, `claude-prompt`, `brightness`, `start-wob`, `argon-battery` scripts |

## Media keys

| Key | Action |
|-----|--------|
| **Fn+F2** | Brightness down (DDC via ddcutil) |
| **Fn+F3** | Brightness up |
| **Fn+F6** | Mute/unmute |
| **Fn+F7** | Volume down |
| **Fn+F8** | Volume up |
| **Battery key** | Open Argon battery dashboard |

All media keys show a visual indicator via wob (Wayland Overlay Bar). Brightness adjusts instantly via gamma, with the panel backlight catching up ~1s later via DDC.

## sway-help

The help overlay (`bin/sway-help`) parses your sway config every time it runs, so it always reflects your current keybindings. Access it via:

- **Mod+Shift+H** (keyboard shortcut)
- Click the keyboard icon in waybar

Type to filter, Escape to dismiss.

## Claude Code integration

Launch Claude Code directly from Sway:

| Binding | Action |
|---------|--------|
| **Mod+C** | Open Claude in a foot terminal |
| **Mod+Shift+C** | Quick prompt — wofi popup, type a question, Claude opens with it |
| **Waybar icon** | Left-click opens Claude, right-click opens quick prompt |

`claude-prompt` opens a minimal wofi input, takes your question, and launches Claude in foot with that prompt. The terminal stays open after Claude responds so you can continue the conversation.

---

## Initial setup from scratch

This section covers setting up a fresh Argon ONE UP CM5 laptop from a clean Raspberry Pi OS install to a working Sway desktop with this config.

### 1. Flash and boot Raspberry Pi OS

Flash **Raspberry Pi OS** (Debian Trixie-based, 64-bit) to your NVMe or SD card using Raspberry Pi Imager. Boot and log in.

### 2. Update the system

```bash
sudo apt update
sudo apt full-upgrade -y
sudo reboot
```

### 3. Fix NVMe power management (critical)

The Argon ONE UP's NVMe drive can become extremely sluggish or cause I/O timeouts without these kernel parameters. This was the single biggest stability issue during initial setup — the system was nearly unusable without this fix.

Edit the kernel command line (**keep everything on one line**):

```bash
sudo nano /boot/firmware/cmdline.txt
```

Append to the existing line:

```
nvme_core.default_ps_max_latency_us=0 pcie_aspm=off
```

This disables NVMe power state transitions and PCIe Active State Power Management, both of which cause latency spikes on the CM5's PCIe bus.

Reboot, then verify no NVMe errors:

```bash
sudo reboot
dmesg -T | grep -i nvme
```

### 4. Install the Argon config tool

This provides battery monitoring, fan control, and power button configuration:

```bash
curl https://download.argon40.com/argononeup.sh | bash
```

### 5. Set Wi-Fi regulatory domain

Incorrect settings can cause poor Wi-Fi performance and channel restrictions:

```bash
sudo raspi-config
```

Navigate to **Localisation Options > WLAN Country** and set your country code (e.g. US).

Verify with:

```bash
iw reg get
```

### 6. Enable seat management

Sway needs proper seat access to manage the display and input devices:

```bash
sudo apt install -y seatd
sudo systemctl enable --now seatd
sudo usermod -aG seat,video,audio,input,render "$USER"
sudo reboot
```

Verify after reboot:

```bash
groups  # should include seat, video, audio, input, render
systemctl status seatd  # should be active
```

### 7. Install dependencies

Core packages:

```bash
sudo apt install -y \
  sway swaybg swayidle swaylock xwayland \
  waybar wofi foot wob mako-notifier \
  grim slurp wl-clipboard \
  ddcutil pipewire wireplumber \
  network-manager network-manager-gnome \
  seatd policykit-1 \
  fonts-jetbrains-mono
```

Optional but recommended:

```bash
sudo apt install -y \
  firefox-esr thunar mpv imv file-roller
```

### 8. Install Rust toolchain and wl-gammarelay-rs

The hybrid brightness control needs `wl-gammarelay-rs` for instant gamma adjustment:

```bash
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
source ~/.cargo/env
cargo install wl-gammarelay-rs
```

Note: compiling on the CM5 takes several minutes.

### 9. Copy this config

```bash
# Clone the repo
git clone https://github.com/jasonwitty/sway-argon-one-up.git
cd sway-argon-one-up

# Copy configs
cp -r sway waybar wob wofi foot mako swaylock gtk-3.0 ~/.config/
mkdir -p ~/.local/bin
cp bin/* ~/.local/bin/
chmod +x ~/.local/bin/*
```

### 10. Test Sway

Log out of any existing desktop session and select **Sway** from the session menu in GDM (the login screen). If everything is set up correctly, you should see the Catppuccin-themed desktop with waybar at the top.

If Sway fails to start, check:

```bash
journalctl -b --no-pager | tail -200
systemctl status seatd
groups  # make sure seat group is present
```

### 11. Install Claude Code (optional)

For the Mod+C integration:

```bash
# See https://claude.ai/claude-code for installation
claude  # run once to authenticate
```

---

## Troubleshooting

### System is extremely slow / NVMe timeouts

The most common issue. Check that the kernel parameters are set:

```bash
cat /proc/cmdline | grep nvme_core
```

If `nvme_core.default_ps_max_latency_us=0` and `pcie_aspm=off` are not present, see step 3 above.

### Brightness keys don't work

Check that `ddcutil` can see the display and `wl-gammarelay-rs` is running:

```bash
ddcutil detect
ddcutil getvcp 10
pgrep wl-gammarelay
```

### No sound / volume keys don't work

This config uses PipeWire with WirePlumber (`wpctl`), not PulseAudio (`pactl`):

```bash
wpctl status
wpctl get-volume @DEFAULT_AUDIO_SINK@
```

### Waybar or wob missing after sway reload

These use `exec_always` and should survive reloads. If they don't appear, check the processes:

```bash
pgrep waybar
pgrep wob
```

And restart manually if needed: `swaymsg reload`
