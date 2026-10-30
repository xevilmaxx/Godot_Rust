//! Port of `core/os/os.h` (query subset) — no windowing (see `display.rs`).

use std::time::Instant;

static START: std::sync::OnceLock<Instant> = std::sync::OnceLock::new();
fn start() -> &'static Instant {
    START.get_or_init(Instant::now)
}

pub struct OS;

impl OS {
    pub fn name() -> &'static str {
        std::env::consts::OS
    }
    pub fn args() -> Vec<String> {
        std::env::args().collect()
    }
    pub fn env(key: &str) -> String {
        std::env::var(key).unwrap_or_default()
    }
    pub fn ticks_msec() -> u64 {
        start().elapsed().as_millis() as u64
    }
    pub fn ticks_usec() -> u64 {
        start().elapsed().as_micros() as u64
    }
    pub fn processor_count() -> usize {
        std::thread::available_parallelism().map(|n| n.get()).unwrap_or(1)
    }
    pub fn unique_id() -> String {
        // Stable per-machine-ish id (hostname + user); good enough for editor prefs.
        format!("{}@{}", Self::env("USERNAME"), Self::env("COMPUTERNAME"))
    }
}
