//! Minimal UTC timestamp formatting (no `time`/`chrono` dependency).

use std::time::{SystemTime, UNIX_EPOCH};

/// Broken-down UTC time.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct UtcTime {
    pub year: i64,
    pub month: u32,
    pub day: u32,
    pub hour: u32,
    pub minute: u32,
    pub second: u32,
    pub millis: u32,
}

impl UtcTime {
    pub(crate) fn now() -> Self {
        Self::from_system_time(SystemTime::now())
    }

    pub(crate) fn from_system_time(time: SystemTime) -> Self {
        let (secs, millis) = match time.duration_since(UNIX_EPOCH) {
            Ok(d) => (d.as_secs() as i64, d.subsec_millis()),
            Err(e) => (-(e.duration().as_secs() as i64), 0),
        };
        Self::from_unix(secs, millis)
    }

    pub(crate) fn from_unix(secs: i64, millis: u32) -> Self {
        let days = secs.div_euclid(86_400);
        let rem = secs.rem_euclid(86_400) as u32;
        let (year, month, day) = civil_from_days(days);
        Self {
            year,
            month,
            day,
            hour: rem / 3600,
            minute: rem % 3600 / 60,
            second: rem % 60,
            millis,
        }
    }

    /// `20261120-153012` — sortable, safe in file names on every platform.
    pub(crate) fn file_stamp(&self) -> String {
        format!(
            "{:04}{:02}{:02}-{:02}{:02}{:02}",
            self.year, self.month, self.day, self.hour, self.minute, self.second
        )
    }

    /// RFC 3339, e.g. `2026-11-20T15:30:12.345Z`.
    pub(crate) fn rfc3339(&self) -> String {
        format!(
            "{:04}-{:02}-{:02}T{:02}:{:02}:{:02}.{:03}Z",
            self.year, self.month, self.day, self.hour, self.minute, self.second, self.millis
        )
    }
}

/// Howard Hinnant's `civil_from_days`: days since 1970-01-01 → (y, m, d).
fn civil_from_days(z: i64) -> (i64, u32, u32) {
    let z = z + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    (if m <= 2 { y + 1 } else { y }, m, d)
}

/// Calendar date and time in UTC for item metadata shown in panels.
pub fn utc_label(time: SystemTime) -> String {
    let t = UtcTime::from_system_time(time);
    format!(
        "{:04}-{:02}-{:02} {:02}:{:02}:{:02} UTC",
        t.year, t.month, t.day, t.hour, t.minute, t.second
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn known_dates() {
        assert_eq!(
            UtcTime::from_unix(0, 0).rfc3339(),
            "1970-01-01T00:00:00.000Z"
        );
        // 2000-02-29 (leap day) 12:34:56
        assert_eq!(
            UtcTime::from_unix(951_827_696, 7).rfc3339(),
            "2000-02-29T12:34:56.007Z"
        );
        // 2026-11-20 15:30:12
        let t = UtcTime::from_unix(1_795_188_612, 0);
        assert_eq!(t.file_stamp(), "20261120-153012");
        // Before the epoch.
        assert_eq!(
            UtcTime::from_unix(-1, 0).rfc3339(),
            "1969-12-31T23:59:59.000Z"
        );
    }
}
