//! Disable sway touchpad while typing.
//!
//! Works around DWT (disable-while-typing) not functioning on USB combo
//! keyboard/touchpad devices like the AMIRA in the Argon ONE UP case.
//! Watches all matching keyboard evdev nodes (the AMIRA exposes the same
//! vid:pid on two USB interfaces) and disables `type:touchpad` via swaymsg
//! while any keys are pressed, re-enabling 150ms after all keys are released.
//!
//! Unlike the earlier Python version, this tracks per-key press/release
//! state rather than "time since last event." The Python script treated
//! autorepeat (value=2) the same as a press, so any dropped release event
//! caused kernel-level autorepeat to keep resetting its timer indefinitely
//! — leaving the touchpad stuck disabled until another key was pressed.
//! This version ignores autorepeat for state tracking, and a 2s "no events
//! at all" safety net force-clears state if a release really did go missing.

use evdev::{Device, EventSummary, KeyCode};
use std::collections::HashSet;
use std::process::Command;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, RecvTimeoutError};
use std::sync::Arc;
use std::thread;
use std::time::{Duration, Instant};

const VENDOR_ID: u16 = 0x6080;
const KEYBOARD_NAME: &str = "AMIRA-KEYBOAR USB KEYBOARD";
// Note: the AMIRA exposes two USB interfaces with different PIDs (0x8060
// and 0x8061). We match on vendor + exact name so we catch both — name
// equality still excludes the Mouse/Touchpad/System Control/Consumer
// Control/Wireless Radio Control subsystem nodes that share the vid.

/// Grace window after the last key release before re-enabling the touchpad.
const GRACE: Duration = Duration::from_millis(150);

/// Keys we track in the pressed set but which never on their own disable
/// the touchpad. Holding Super to switch workspaces, Shift to select, Ctrl
/// for a shortcut, etc. isn't "typing" — no palm rest to guard against —
/// and the AMIRA drops release events on modifiers frequently enough that
/// treating modifier-only holds as typing locks the touchpad out for the
/// full STUCK_TIMEOUT every time a modifier press is mishandled.
fn is_modifier(key: KeyCode) -> bool {
    matches!(
        key,
        KeyCode::KEY_LEFTCTRL
            | KeyCode::KEY_RIGHTCTRL
            | KeyCode::KEY_LEFTSHIFT
            | KeyCode::KEY_RIGHTSHIFT
            | KeyCode::KEY_LEFTALT
            | KeyCode::KEY_RIGHTALT
            | KeyCode::KEY_LEFTMETA
            | KeyCode::KEY_RIGHTMETA
    )
}

/// If any key is marked pressed but no events at all have arrived for this
/// long, assume a release was dropped and force-clear state.
const STUCK_TIMEOUT: Duration = Duration::from_secs(2);

/// Retry interval when no matching keyboards are found at startup.
const DISCOVER_INTERVAL: Duration = Duration::from_secs(2);

enum Msg {
    KeyEvent { key: KeyCode, value: i32 },
    ReaderDied,
    Shutdown,
}

fn matches(device: &Device) -> bool {
    device.input_id().vendor() == VENDOR_ID && device.name() == Some(KEYBOARD_NAME)
}

fn find_keyboards() -> Vec<(std::path::PathBuf, Device)> {
    evdev::enumerate()
        .filter(|(_, d)| matches(d))
        .collect()
}

fn spawn_reader(mut device: Device, tx: mpsc::Sender<Msg>) {
    thread::spawn(move || loop {
        match device.fetch_events() {
            Ok(events) => {
                for ev in events {
                    if let EventSummary::Key(_, key, value) = ev.destructure() {
                        if tx.send(Msg::KeyEvent { key, value }).is_err() {
                            return;
                        }
                    }
                }
            }
            Err(_) => {
                let _ = tx.send(Msg::ReaderDied);
                return;
            }
        }
    });
}

fn swaymsg(state: &'static str) {
    let _ = Command::new("swaymsg")
        .args(["input", "type:touchpad", "events", state])
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn();
}

fn main() {
    let (tx, rx) = mpsc::channel::<Msg>();

    // Signal handler: send shutdown on SIGTERM/SIGINT so the main loop
    // exits cleanly and re-enables the touchpad.
    let shutdown_flag = Arc::new(AtomicBool::new(false));
    {
        let tx = tx.clone();
        let flag = shutdown_flag.clone();
        ctrlc::set_handler(move || {
            // First signal: request clean shutdown. Second signal: exit hard
            // so we don't hang if something's wedged.
            if flag.swap(true, Ordering::SeqCst) {
                std::process::exit(130);
            }
            let _ = tx.send(Msg::Shutdown);
        })
        .expect("failed to install signal handler");
    }

    // Initial discovery (retry until something shows up).
    let keyboards = loop {
        let found = find_keyboards();
        if !found.is_empty() {
            break found;
        }
        eprintln!("trackpad-guard: no matching keyboards found, retrying in 2s");
        thread::sleep(DISCOVER_INTERVAL);
        if shutdown_flag.load(Ordering::SeqCst) {
            return;
        }
    };

    eprintln!(
        "trackpad-guard: watching {} keyboard(s)",
        keyboards.len()
    );
    let mut readers_alive = keyboards.len();
    for (_, device) in keyboards {
        spawn_reader(device, tx.clone());
    }

    // Make sure we start from a known-enabled state in case a previous run
    // crashed while the touchpad was disabled.
    swaymsg("enabled");

    let mut pressed: HashSet<KeyCode> = HashSet::new();
    let mut disabled = false;
    let mut last_release: Option<Instant> = None;
    // Time of the most recent press or release. Autorepeat (value=2) does
    // NOT update this — that's the whole point. When a release is dropped,
    // the kernel keeps firing autorepeats at ~33Hz, so we can't detect a
    // stuck key by "no events for a while"; we have to detect "only
    // autorepeats for a while."
    let mut last_transition = Instant::now();
    let debug = std::env::var_os("TRACKPAD_GUARD_DEBUG").is_some();

    'main: loop {
        let timeout = if !disabled {
            Duration::from_secs(3600)
        } else if !pressed.is_empty() {
            // Waiting to detect that only autorepeats have arrived.
            STUCK_TIMEOUT.saturating_sub(last_transition.elapsed()) + Duration::from_millis(1)
        } else if let Some(t) = last_release {
            GRACE.saturating_sub(t.elapsed()) + Duration::from_millis(1)
        } else {
            Duration::from_millis(50)
        };

        match rx.recv_timeout(timeout) {
            Ok(Msg::KeyEvent { key, value }) => match value {
                1 => {
                    pressed.insert(key);
                    last_transition = Instant::now();
                    last_release = None;
                    // Only non-modifiers trigger the disable. Modifiers stay
                    // in the `pressed` set (so "still typing" stays accurate
                    // while they're held) but never on their own flip the
                    // touchpad off.
                    if !disabled && !is_modifier(key) {
                        swaymsg("disabled");
                        disabled = true;
                        if debug {
                            eprintln!("trackpad-guard: disabled (press {key:?})");
                        }
                    }
                }
                0 => {
                    pressed.remove(&key);
                    last_transition = Instant::now();
                    // Start the grace timer when no non-modifier keys remain
                    // (typing stopped). Dangling modifier holds don't keep
                    // the touchpad off.
                    if pressed.iter().all(|k| is_modifier(*k)) {
                        last_release = Some(Instant::now());
                    }
                }
                // value=2 is autorepeat — the key is still logically held.
                // Deliberately NOT updating last_transition: that's how we
                // distinguish "keys really being held" from "missed release
                // leaving the kernel autorepeating forever."
                _ => {}
            },
            Ok(Msg::ReaderDied) => {
                readers_alive = readers_alive.saturating_sub(1);
                if readers_alive == 0 {
                    eprintln!("trackpad-guard: all keyboards disconnected, exiting");
                    break 'main;
                }
            }
            Ok(Msg::Shutdown) => break 'main,
            Err(RecvTimeoutError::Timeout) => {}
            Err(RecvTimeoutError::Disconnected) => break 'main,
        }

        if disabled {
            // Stuck-key safety net: if any key is marked pressed but we've
            // seen only autorepeats (no real press or release) for
            // STUCK_TIMEOUT, a release was dropped by the kernel/USB layer.
            // Force-clear and let the grace timer re-enable.
            if !pressed.is_empty() && last_transition.elapsed() >= STUCK_TIMEOUT {
                eprintln!(
                    "trackpad-guard: autorepeat-only for {:?}, clearing stuck state ({} key(s): {:?})",
                    STUCK_TIMEOUT,
                    pressed.len(),
                    pressed,
                );
                pressed.clear();
                last_release = Some(Instant::now());
            }

            // Grace period expired and no non-modifiers remain pressed —
            // re-enable. Held modifiers alone don't count as typing.
            if pressed.iter().all(|k| is_modifier(*k)) {
                if let Some(t) = last_release {
                    if t.elapsed() >= GRACE {
                        swaymsg("enabled");
                        disabled = false;
                        last_release = None;
                        if debug {
                            eprintln!("trackpad-guard: enabled (grace elapsed)");
                        }
                    }
                }
            }
        }
    }

    // Always re-enable on exit.
    swaymsg("enabled");
}
