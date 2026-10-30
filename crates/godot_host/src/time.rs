//! Port of `core/os/time.h` (date/time + benchmarks).

pub struct Time;

impl Time {
    pub fn ticks_msec() -> u64 {
        super::os::OS::ticks_msec()
    }
    /// Seconds since Unix epoch (system clock).
    pub fn unix_time() -> f64 {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs_f64())
            .unwrap_or(0.0)
    }
    /// `YYYY-MM-DDTHH:MM:SS` (UTC) — matches `Time::get_datetime_string_from_system`.
    pub fn datetime_string_utc() -> String {
        // Days-based civil-from-days (Howard Hinnant) over Unix epoch, no deps.
        let secs = Self::unix_time() as i64;
        let days = secs.div_euclid(86400);
        let sod = secs.rem_euclid(86400);
        let (y, m, d) = civil_from_days(days + 719468);
        format!("{y:04}-{m:02}-{d:02}T{:02}:{:02}:{:02}", sod / 3600, sod % 3600 / 60, sod % 60)
    }
}

fn civil_from_days(z: i64) -> (i64, u32, u32) {
    let era = z.div_euclid(146097);
    let doe = z.rem_euclid(146097);
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    (if m <= 2 { y + 1 } else { y }, m as u32, d as u32)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn clock_advances() {
        assert!(Time::unix_time() > 1_700_000_000.0);
        assert_eq!(&Time::datetime_string_utc()[4..5], "-");
    }
}
