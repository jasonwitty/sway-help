//! Lightweight battery drain logger for lid-close power testing.
//!
//! Usage:
//!   lid-power-test start [--interval N] [--label NAME]
//!   lid-power-test stop
//!   lid-power-test read
//!   lid-power-test summary <logfile>

use i2cdev::core::I2CDevice as _;
use i2cdev::linux::LinuxI2CDevice;
use std::sync::atomic::{AtomicBool, Ordering};
use std::{env, fs, io::Write, path::PathBuf, process, thread, time};

const I2C_BUS: &str = "/dev/i2c-1";
const ADDR_BATTERY: u16 = 0x64;
const SOC_HIGH_REG: u8 = 0x04;
const SOC_LOW_REG: u8 = 0x05;
const CURRENT_HIGH_REG: u8 = 0x0E;
const CURRENT_LOW_REG: u8 = 0x0F;
const TEMP_PATH: &str = "/sys/class/thermal/thermal_zone0/temp";

const PID_FILE: &str = "/dev/shm/lid-power-test.pid";
const LOGPATH_FILE: &str = "/dev/shm/lid-power-test.logpath";
const DEFAULT_INTERVAL: u64 = 30;

static RUNNING: AtomicBool = AtomicBool::new(true);

extern "C" fn handle_signal(_: libc::c_int) {
    RUNNING.store(false, Ordering::SeqCst);
}

// ── Battery & Sensor Reads ──────────────────────────────────────────

fn read_battery() -> Option<(f64, i16)> {
    let mut dev = LinuxI2CDevice::new(I2C_BUS, ADDR_BATTERY).ok()?;
    let soc_h = dev.smbus_read_byte_data(SOC_HIGH_REG).ok()?;
    let soc_l = dev.smbus_read_byte_data(SOC_LOW_REG).ok()?;
    let cur_h = dev.smbus_read_byte_data(CURRENT_HIGH_REG).ok()?;
    let cur_l = dev.smbus_read_byte_data(CURRENT_LOW_REG).ok()?;

    let soc = f64::from(soc_h) + f64::from(soc_l) / 256.0;
    let raw_cur = i16::from_be_bytes([cur_h, cur_l]);

    Some((soc, raw_cur))
}

fn read_temp() -> Option<i32> {
    fs::read_to_string(TEMP_PATH)
        .ok()?
        .trim()
        .parse::<i32>()
        .ok()
}

fn capture_pmic() -> String {
    process::Command::new("vcgencmd")
        .arg("pmic_read_adc")
        .output()
        .ok()
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .unwrap_or_default()
}

fn now_epoch() -> u64 {
    time::SystemTime::now()
        .duration_since(time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

fn timestamp_string() -> String {
    // Use date command for human-readable timestamp (no chrono dependency)
    process::Command::new("date")
        .arg("+%Y-%m-%d_%H%M")
        .output()
        .ok()
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .map(|s| s.trim().to_string())
        .unwrap_or_else(|| format!("{}", now_epoch()))
}

fn log_dir() -> PathBuf {
    let home = env::var("HOME").unwrap_or_else(|_| "/tmp".into());
    PathBuf::from(home).join(".local/state/lid-power")
}

// ── Commands ────────────────────────────────────────────────────────

fn cmd_start(args: &[String]) {
    let mut interval = DEFAULT_INTERVAL;
    let mut label = String::from("unlabeled");

    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--interval" => {
                i += 1;
                if let Some(v) = args.get(i) {
                    interval = v.parse().unwrap_or(DEFAULT_INTERVAL);
                }
            }
            "--label" => {
                i += 1;
                if let Some(v) = args.get(i) {
                    label = v.clone();
                }
            }
            _ => {}
        }
        i += 1;
    }

    // Check for existing logger
    if fs::read_to_string(PID_FILE).is_ok() {
        eprintln!("Logger already running ({}). Run 'lid-power-test stop' first.", PID_FILE);
        process::exit(1);
    }

    // Set up signal handlers
    unsafe {
        libc::signal(libc::SIGTERM, handle_signal as *const () as libc::sighandler_t);
        libc::signal(libc::SIGINT, handle_signal as *const () as libc::sighandler_t);
    }

    // Create log directory and file
    let dir = log_dir();
    fs::create_dir_all(&dir).ok();
    let log_path = dir.join(format!("{}.csv", timestamp_string()));
    let log_path_str = log_path.to_string_lossy().to_string();

    // Write PID and log path
    fs::write(PID_FILE, process::id().to_string()).ok();
    fs::write(LOGPATH_FILE, &log_path_str).ok();

    let mut file = match fs::File::create(&log_path) {
        Ok(f) => f,
        Err(e) => {
            eprintln!("Cannot create log: {e}");
            cleanup_pid();
            process::exit(1);
        }
    };

    // Capture starting PMIC snapshot
    let pmic_start = capture_pmic();

    // Write header
    let _ = writeln!(file, "# lid-power-test log");
    let _ = writeln!(file, "# label: {label}");
    let _ = writeln!(file, "# interval: {interval}s");
    let _ = writeln!(file, "# pmic_start:");
    for line in pmic_start.lines() {
        let _ = writeln!(file, "#   {line}");
    }
    let _ = writeln!(file, "epoch,soc,temp_mc,current_raw");

    eprintln!("Logging to {log_path_str} every {interval}s (label: {label})");

    // Logging loop
    while RUNNING.load(Ordering::SeqCst) {
        let epoch = now_epoch();
        let (soc, cur) = read_battery().unwrap_or((-1.0, 0));
        let temp = read_temp().unwrap_or(-1);

        let _ = writeln!(file, "{epoch},{soc:.4},{temp},{cur}");
        let _ = file.flush();

        // Sleep in 1-second ticks so we respond to signals promptly
        for _ in 0..interval {
            if !RUNNING.load(Ordering::SeqCst) {
                break;
            }
            thread::sleep(time::Duration::from_secs(1));
        }
    }

    // Final reading
    let epoch = now_epoch();
    let (soc, cur) = read_battery().unwrap_or((-1.0, 0));
    let temp = read_temp().unwrap_or(-1);
    let _ = writeln!(file, "{epoch},{soc:.4},{temp},{cur}");

    // Capture ending PMIC snapshot
    let pmic_end = capture_pmic();
    let _ = writeln!(file, "# pmic_end:");
    for line in pmic_end.lines() {
        let _ = writeln!(file, "#   {line}");
    }

    let _ = file.flush();
    cleanup_pid();
    eprintln!("Logger stopped.");
}

fn cmd_stop() {
    let pid_str = match fs::read_to_string(PID_FILE) {
        Ok(s) => s.trim().to_string(),
        Err(_) => {
            eprintln!("No logger running (no PID file).");
            process::exit(1);
        }
    };

    // Read log path BEFORE signaling (logger cleans up on exit)
    let log_path = fs::read_to_string(LOGPATH_FILE)
        .ok()
        .map(|s| s.trim().to_string());

    let pid: i32 = match pid_str.parse() {
        Ok(p) => p,
        Err(_) => {
            eprintln!("Invalid PID in {PID_FILE}");
            cleanup_pid();
            process::exit(1);
        }
    };

    // Send SIGTERM
    unsafe {
        libc::kill(pid, libc::SIGTERM);
    }

    // Wait for process to exit (up to 5 seconds)
    for _ in 0..50 {
        let alive = unsafe { libc::kill(pid, 0) } == 0;
        if !alive {
            break;
        }
        thread::sleep(time::Duration::from_millis(100));
    }

    // Print summary
    if let Some(path) = log_path {
        print_summary_from_file(&path);
    } else {
        eprintln!("Log path file not found.");
    }

    cleanup_pid();
}

fn cmd_read() {
    let temp = read_temp().unwrap_or(-1);
    let temp_c = temp as f64 / 1000.0;

    match read_battery() {
        Some((soc, cur)) => {
            let charging = (cur as u16 >> 15) == 0;
            let status = if charging { "charging" } else { "discharging" };
            println!("SOC:     {soc:.4}%");
            println!("Status:  {status}");
            println!("Current: {cur} (raw)");
            println!("Temp:    {temp_c:.1}°C");
        }
        None => {
            eprintln!("Failed to read battery gauge.");
            process::exit(1);
        }
    }

    let pmic = capture_pmic();
    if !pmic.is_empty() {
        println!("\nPMIC rails:");
        print!("{pmic}");
    }
}

fn cmd_summary(path: &str) {
    print_summary_from_file(path);
}

// ── Log Parsing & Summary ───────────────────────────────────────────

struct Entry {
    epoch: u64,
    soc: f64,
    temp_mc: i32,
}

fn parse_log(path: &str) -> (Vec<Entry>, String) {
    let content = match fs::read_to_string(path) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("Cannot read {path}: {e}");
            return (vec![], String::new());
        }
    };

    let mut entries = Vec::new();
    let mut label = String::from("unlabeled");

    for line in content.lines() {
        if line.starts_with('#') {
            if let Some(l) = line.strip_prefix("# label: ") {
                label = l.to_string();
            }
            continue;
        }
        if line.starts_with("epoch,") {
            continue;
        }
        let parts: Vec<&str> = line.split(',').collect();
        if parts.len() >= 3 {
            if let (Ok(epoch), Ok(soc), Ok(temp)) = (
                parts[0].parse::<u64>(),
                parts[1].parse::<f64>(),
                parts[2].parse::<i32>(),
            ) {
                entries.push(Entry { epoch, soc, temp_mc: temp });
            }
        }
    }

    (entries, label)
}

fn print_summary_from_file(path: &str) {
    let (entries, label) = parse_log(path);

    if entries.len() < 2 {
        eprintln!("Not enough data points in log (need at least 2).");
        return;
    }

    let first = &entries[0];
    let last = &entries[entries.len() - 1];
    let duration_s = last.epoch.saturating_sub(first.epoch);
    let soc_delta = first.soc - last.soc;

    let drain_per_hour = if duration_s > 0 {
        soc_delta / (duration_s as f64 / 3600.0)
    } else {
        0.0
    };

    let est_hours = if drain_per_hour > 0.0 {
        100.0 / drain_per_hour
    } else {
        f64::INFINITY
    };

    let min_temp = entries.iter().map(|e| e.temp_mc).min().unwrap_or(0);
    let max_temp = entries.iter().map(|e| e.temp_mc).max().unwrap_or(0);
    let avg_temp: f64 =
        entries.iter().map(|e| e.temp_mc as f64).sum::<f64>() / entries.len() as f64;

    let mins = duration_s / 60;
    let secs = duration_s % 60;

    println!("=== Lid Power Test Results ===");
    println!("Label:        {label}");
    println!("Duration:     {mins}m {secs}s");
    println!("Samples:      {}", entries.len());
    println!("SOC:          {:.4}% → {:.4}%  (Δ {soc_delta:.4}%)", first.soc, last.soc);
    println!("Drain rate:   {drain_per_hour:.2} %/hr");
    if est_hours < 200.0 {
        println!("Est. battery: ~{est_hours:.1} hours (from 100%)");
    }
    println!(
        "Temp:         {:.1}°C avg, {:.1}°C min, {:.1}°C max",
        avg_temp / 1000.0,
        min_temp as f64 / 1000.0,
        max_temp as f64 / 1000.0
    );
    println!("Log:          {path}");

    // Show drain curve in 5-minute buckets if enough data
    if duration_s >= 300 {
        println!("\n--- Drain over time ---");
        let bucket_s = 300u64; // 5-minute buckets
        let mut bucket_start = 0usize;
        let mut t = first.epoch + bucket_s;

        while t <= last.epoch + bucket_s {
            let bucket_end = entries
                .iter()
                .position(|e| e.epoch >= t)
                .unwrap_or(entries.len() - 1);

            if bucket_end > bucket_start {
                let b_first = &entries[bucket_start];
                let b_last = &entries[bucket_end];
                let b_dur = b_last.epoch.saturating_sub(b_first.epoch);
                let b_delta = b_first.soc - b_last.soc;
                let b_rate = if b_dur > 0 {
                    b_delta / (b_dur as f64 / 3600.0)
                } else {
                    0.0
                };
                let offset_min = (b_first.epoch - first.epoch) / 60;
                println!(
                    "  +{offset_min:3}m: {:.4}% → {:.4}%  ({b_rate:.2} %/hr)",
                    b_first.soc, b_last.soc
                );
            }

            bucket_start = bucket_end;
            t += bucket_s;
        }
    }
}

// ── Helpers ─────────────────────────────────────────────────────────

fn cleanup_pid() {
    let _ = fs::remove_file(PID_FILE);
    let _ = fs::remove_file(LOGPATH_FILE);
}

fn print_usage() {
    eprintln!("lid-power-test — battery drain logger for lid-close testing");
    eprintln!();
    eprintln!("Usage:");
    eprintln!("  lid-power-test start [--interval N] [--label NAME]");
    eprintln!("  lid-power-test stop");
    eprintln!("  lid-power-test read");
    eprintln!("  lid-power-test summary <logfile>");
    eprintln!();
    eprintln!("Integration with lid-suspend:");
    eprintln!("  close)  lid-power-test start --label \"test-name\" &");
    eprintln!("  open)   lid-power-test stop");
}

fn main() {
    let args: Vec<String> = env::args().collect();

    match args.get(1).map(|s| s.as_str()) {
        Some("start") => cmd_start(&args[2..]),
        Some("stop") => cmd_stop(),
        Some("read") => cmd_read(),
        Some("summary") => {
            if let Some(path) = args.get(2) {
                cmd_summary(path);
            } else {
                eprintln!("Usage: lid-power-test summary <logfile>");
                process::exit(1);
            }
        }
        _ => print_usage(),
    }
}
