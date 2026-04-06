# Usage Manual

A reference guide for using the Sway desktop on Argon ONE UP. Covers keyboard shortcuts, theme switching, power management, and configuration.

## Table of Contents

- [Keyboard Shortcuts](#keyboard-shortcuts)
- [Help Menu](#help-menu)
- [Theme Switching](#theme-switching)
- [Battery and Power Management](#battery-and-power-management)
- [Display Scaling](#display-scaling)
- [Bazaar Flatpak Store](#bazaar-flatpak-store)
- [Web Apps](#web-apps)
- [Screen Recording](#screen-recording)
- [Configuration Files](#configuration-files)

---

## Keyboard Shortcuts

`Mod` is the Super/Windows key.

### General

| Shortcut | Action |
|----------|--------|
| **Mod+Enter** | Open terminal (foot) |
| **Mod+D** | App launcher (wofi) |
| **Ctrl+Shift+Enter** | App launcher (alternate) |
| **Mod+Q** | Close focused window |
| **Mod+Shift+R** | Reload sway config |
| **Mod+Shift+E** | Exit sway (logout prompt) |

### Navigation

| Shortcut | Action |
|----------|--------|
| **Mod+H/J/K/L** | Focus left / down / up / right |
| **Mod+Arrow keys** | Focus left / down / up / right |
| **Mod+1–0** | Switch to workspace 1–10 |
| **Mod+Shift+1–0** | Move window to workspace 1–10 |

### Window Management

| Shortcut | Action |
|----------|--------|
| **Mod+Shift+H/J/K/L** | Move window left / down / up / right |
| **Mod+Shift+Arrow keys** | Move window left / down / up / right |
| **Mod+F** | Toggle fullscreen |
| **Mod+Shift+V** | Split horizontal |
| **Mod+V** | Split vertical |
| **Mod+S** | Stacking layout |
| **Mod+W** | Tabbed layout |
| **Mod+E** | Toggle split layout |
| **Mod+Shift+Space** | Toggle floating mode |
| **Mod+Space** | Focus tiling / floating |
| **Mod+A** | Focus parent container |
| **Mod+R** | Enter resize mode (H/J/K/L to resize, Esc to exit) |

### Scratchpad

| Shortcut | Action |
|----------|--------|
| **Mod+Shift+Minus** | Send window to scratchpad |
| **Mod+Minus** | Show/cycle scratchpad windows |

### Applications

| Shortcut | Action |
|----------|--------|
| **Mod+B** | Launch Brave browser |
| **Mod+N** | Open file manager (Thunar) |
| **Mod+C** | Open Claude Code |
| **Mod+Shift+C** | Quick Claude prompt (wofi popup) |
| **Mod+T** | Theme picker |
| **Mod+Shift+H** | Keybinding help overlay |
| **Mod+=** | Calculator (galculator) |

### Media Keys

| Key | Action |
|-----|--------|
| **Fn+F2** | Brightness down |
| **Fn+F3** | Brightness up |
| **Fn+F6** | Mute / unmute |
| **Fn+F7** | Volume down (5%) |
| **Fn+F8** | Volume up (5%) |
| **Battery key** | Argon battery dashboard |

### Screenshots

| Shortcut | Action |
|----------|--------|
| **Print** | Full screenshot (saved to ~/Pictures) |
| **Mod+Print** | Area screenshot to clipboard |
| **Mod+Shift+S** | Area screenshot to clipboard (alternate) |

---

## Help Menu

Press **Mod+Shift+H** or click the keyboard icon in the waybar to open the help overlay. It parses your live sway config every time it runs, so it always reflects your current keybindings. Type to filter, press Escape to dismiss.

---

## Theme Switching

Nine themes are available, each applying a coordinated color scheme across all apps simultaneously — sway window borders, waybar, foot terminals, mako notifications, swaylock, wofi, wob, GTK apps (including Thunar folder colors), Brave, and Chromium.

**Available themes:** Catppuccin Frappe, Mocha, Latte, Macchiato, Dracula, Nord, Gruvbox Dark, Monokai Dark, Monokai Light.

| Method | Action |
|--------|--------|
| **Mod+T** | Open theme picker (wofi) |
| **Waybar palette icon** | Open theme picker |
| `switch-theme <name>` | Switch directly (e.g., `switch-theme dracula`) |
| `switch-theme --wallpaper-picker` | Choose a wallpaper (overrides theme default) |
| `switch-theme --wallpaper <path>` | Set a specific wallpaper |
| `switch-theme --wallpaper-reset` | Revert to the current theme's default wallpaper |

Foot terminals are recolored live via OSC escape sequences — no restart needed. Browsers update live via managed policy files — no restart needed.

---

## Battery and Power Management

### Battery Status

The Argon ONE UP has its own battery, separate from the Pi's power supply. The battery icon in waybar shows the current charge level and whether it's charging. The battery is monitored by `argon-battery-rs`, a purpose-built Rust binary that reads the fuel gauge IC directly over I2C.

### Automatic Power Behavior

On login, the `power-startup` script detects whether the laptop is on AC or battery and sets:

| State | Brightness | CPU Governor |
|-------|-----------|-------------|
| **AC power** | 100% | ondemand (scales with demand) |
| **Battery** | 40% | powersave (minimum frequency) |

When you plug/unplug the charger, `argon-battery-rs` detects the transition and automatically adjusts brightness and governor to match.

### Lid Close

Closing the lid triggers a soft suspend (the Pi does not support real suspend):

- Screen locks (swaylock)
- Display turns off
- CPU governor switches to powersave
- WiFi and Bluetooth are blocked
- Webcam is unbound from USB

All are reversed when the lid opens. WiFi reconnects automatically.

### Changing Power Profile

Click the battery icon in waybar or run `power-mode toggle` to cycle through CPU governors:

| Profile | Governor | Behavior |
|---------|----------|----------|
| **Balanced** | ondemand | CPU scales frequency with demand |
| **Powersave** | powersave | CPU stays at minimum frequency |
| **Performance** | performance | CPU stays at maximum frequency |

You can also set a specific profile: `power-mode powersave`, `power-mode performance`, `power-mode ondemand`.

### Brightness Controls

Brightness is controlled via DDC/CI over I2C bus 14, achieving ~1ms response time (much faster than standard backlight controls).

| Method | Action |
|--------|--------|
| **Fn+F2 / Fn+F3** | Brightness down / up (5% steps) |
| `brightness up` / `brightness down` | Manual adjustment from terminal |

Brightness range: 5% – 100%. Current level is cached in `/tmp/.brightness_level`.

---

## Display Scaling

Adjust Sway's output scale to balance screen real estate vs readability on the 1920x1200 panel.

| Method | Action |
|--------|--------|
| **Waybar magnifier icon** | Open scale picker (wofi) |

Available steps: 1x, 1.25x, 1.5x, 1.6x (default), 1.75x, 2x.

The default is 1.6x, giving an effective resolution of 1200x750.

---

## Bazaar Flatpak Store

If you installed Bazaar (Flatpak app store) during setup, you can browse and install Flatpak apps through its graphical interface. Launch it from the app launcher (Mod+D, search for "Bazaar").

Flatpak apps run sandboxed and are architecture-independent, which is useful on ARM where not every app has a native .deb package.

---

## Web Apps

If you installed WebApps (Linux Mint webapp-manager) during setup, you can pin websites as standalone windows with their own app icons — no browser tabs needed.

To create a web app:

1. Launch **Web Apps** from the app launcher (Mod+D)
2. Enter the URL (e.g., `https://slack.com`)
3. Give it a name and optionally choose an icon
4. Select which browser to use (Brave or Chromium)
5. Click **Install**

The web app will appear in your app launcher like any other application. This is especially useful for services like Slack, Teams, or other web-based tools that don't have native ARM packages.

---

## Screen Recording

A toggle-based screen recorder using `wf-recorder`:

- First run starts recording, second run stops it
- Recordings are saved to `~/Videos/` as MP4
- A notification confirms when recording starts and stops

Usage from terminal: `screen-record` (full screen) or `screen-record area` (select a region).

---

## Configuration Files

These are the files you're most likely to want to customize:

| File | What it controls |
|------|-----------------|
| `~/.config/sway/config` | Keyboard shortcuts, autostart apps, gaps, borders, output scale |
| `~/.config/waybar/config` | Waybar modules, layout, click actions |
| `~/.config/foot/foot.ini` | Terminal font, size, padding |
| `~/.config/wofi/config` | App launcher dimensions, behavior |
| `~/.config/sway-themes/current` | Active theme name (text file) |
| `~/.config/sway-themes/<theme>` | Theme color definitions (35 color variables + wallpaper path) |
| `~/.config/sway-themes/templates/*` | Templates with `@@VARIABLE@@` placeholders rendered by switch-theme |
| `~/.config/mako/config` | Notification style, timeout, position |
| `~/.config/swaylock/config` | Lock screen appearance |
| `~/.config/gtk-3.0/settings.ini` | GTK theme, icon theme, font |
| `~/.config/fish/config.fish` | Shell aliases, environment variables, prompt behavior |
| `~/.config/starship.toml` | Starship prompt appearance |
| `~/.wallpapers/` | Wallpaper images (add your own here) |
| `~/.local/bin/` | All custom scripts (brightness, lid-suspend, power-mode, etc.) |
| `/etc/argononeupd.conf` | Argon daemon lid action and fan settings |
