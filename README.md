# sway-help

Sway window manager configuration with Catppuccin Frappe theme and a dynamic keybinding help overlay.

## What's included

| Directory | Description |
|-----------|-------------|
| `sway/` | Sway config with Catppuccin Frappe window colors, idle lock, touchpad, NetworkManager |
| `waybar/` | Top bar with workspaces, clock, volume, network, backlight, battery, help + power buttons |
| `wofi/` | App launcher and help overlay styles |
| `foot/` | Terminal emulator with Frappe 16-color palette |
| `mako/` | Notification daemon themed to match |
| `swaylock/` | Lock screen with Frappe colored ring indicator |
| `gtk-3.0/` | GTK dark theme settings |
| `bin/` | `sway-help` — parses sway config live and displays keybindings in a searchable wofi popup |

## sway-help

The help overlay (`bin/sway-help`) parses your sway config every time it runs, so it always reflects your current keybindings. Access it via:

- **Mod+Shift+H** (keyboard shortcut)
- Click the keyboard icon in waybar

Type to filter, Escape to dismiss.

## Install

```bash
# Copy configs
cp -r sway waybar wofi foot mako swaylock gtk-3.0 ~/.config/
cp bin/sway-help ~/.local/bin/
chmod +x ~/.local/bin/sway-help

# Dependencies
sudo apt install sway waybar wofi foot mako-notifier swaylock swayidle \
  grim slurp wl-clipboard network-manager-gnome brightnessctl
```

## Dependencies

- sway, waybar, wofi, foot, mako, swaylock, swayidle
- grim, slurp, wl-clipboard (screenshots)
- network-manager-gnome (nm-applet)
- brightnessctl
- JetBrainsMono Nerd Font
