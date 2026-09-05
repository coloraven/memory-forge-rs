use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};

static ENABLED: AtomicBool = AtomicBool::new(false);
static LOG_PATH: Mutex<Option<PathBuf>> = Mutex::new(None);

pub fn configure(enabled: bool, data_dir: &Path) {
    let logs_dir = data_dir.join("logs");
    let path = logs_dir.join("memory-forge.log");
    if enabled {
        let _ = fs::create_dir_all(&logs_dir);
    }
    if let Ok(mut guard) = LOG_PATH.lock() {
        *guard = Some(path.clone());
    }
    ENABLED.store(enabled, Ordering::Relaxed);
    if enabled {
        write_line("=== program logging enabled ===");
    }
}

pub fn is_enabled() -> bool {
    ENABLED.load(Ordering::Relaxed)
}

pub fn log_file_path() -> Option<PathBuf> {
    LOG_PATH.lock().ok().and_then(|guard| guard.clone())
}

pub fn log_dir_path() -> Option<PathBuf> {
    log_file_path().and_then(|path| path.parent().map(Path::to_path_buf))
}

/// Append a timestamped line when logging is enabled. Also mirrors to stderr.
pub fn write_line(message: &str) {
    if !is_enabled() {
        return;
    }
    let ts = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| {
            let secs = d.as_secs();
            let millis = d.subsec_millis();
            format!("{secs}.{millis:03}")
        })
        .unwrap_or_else(|_| "0".to_string());
    let line = format!("[{ts}] {message}");
    eprintln!("{line}");
    let Ok(guard) = LOG_PATH.lock() else {
        return;
    };
    let Some(path) = guard.as_ref() else {
        return;
    };
    if let Some(parent) = path.parent() {
        let _ = fs::create_dir_all(parent);
    }
    if let Ok(mut file) = OpenOptions::new().create(true).append(true).open(path) {
        let _ = writeln!(file, "{line}");
    }
}

pub fn perf(message: impl AsRef<str>) {
    write_line(&format!("[perf] {}", message.as_ref()));
}
