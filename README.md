# sway-argon-one-up

Sway window manager configuration for the [Argon ONE UP CM5 Laptop](https://argon40.com/products/argon-one-up-cm5-laptop-core-system), a 14-inch laptop powered by the Raspberry Pi Compute Module 5. Includes Catppuccin Frappe theming, a dynamic keybinding help overlay, DDC brightness control over HDMI, and Claude Code integration.

![screenshot](screenshot.png)

## Hardware

This config is built for the [Argon ONE UP CM5 Laptop](https://argon40.com/products/argon-one-up-cm5-laptop-core-system) which uses a Raspberry Pi Compute Module 5. The display is connected via HDMI internally, so standard backlight controls don't apply — brightness is handled through DDC (`ddcutil`). The Argon case also has its own battery, monitored via a custom script.

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

All media keys show a visual indicator via wob (Wayland Overlay Bar).

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

## Prerequisites

### Argon config tool

Install the Argon ONE UP configuration tool for battery monitoring and fan control:

```bash
curl https://download.argon40.com/argononeup.sh | bash
```

### Dependencies

```bash
sudo apt install sway waybar wofi foot mako-notifier swaylock swayidle \
  grim slurp wl-clipboard wob ddcutil pipewire wireplumber \
  network-manager-gnome fonts-jetbrains-mono
```

[Claude Code](https://claude.ai/claude-code) must be installed separately for the Mod+C integration.

## Install

```bash
# Copy configs
cp -r sway waybar wob wofi foot mako swaylock gtk-3.0 ~/.config/
cp bin/* ~/.local/bin/
chmod +x ~/.local/bin/sway-help ~/.local/bin/claude-prompt \
  ~/.local/bin/brightness ~/.local/bin/start-wob ~/.local/bin/argon-battery
```
