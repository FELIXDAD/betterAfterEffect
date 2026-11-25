use super::*;
use chrono::{Datelike, Duration, Local, NaiveDate, Timelike};

fn smpte(frame: i64, rate: i64, drop: bool) -> String {
    let negative = frame < 0;
    let mut n = frame.unsigned_abs() as i64;
    let skipped = if drop && matches!(rate, 30 | 60) {
        rate / 15
    } else {
        0
    };
    if skipped > 0 {
        let minute = rate * 60 - skipped;
        let ten = rate * 600 - 9 * skipped;
        n += skipped * (9 * (n / ten) + (n % ten - skipped).max(0) / minute);
    }
    let s = n / rate;
    format!(
        "{}{:02}:{:02}:{:02}{}{:02}",
        if negative { "-" } else { "" },
        s / 3600,
        (s / 60) % 60,
        s % 60,
        if skipped > 0 { ";" } else { ":" },
        n % rate
    )
}

pub(super) fn timecode(v: Params<'_>) -> String {
    let p = v.p;
    let custom = v.popup(9) == 3;
    let rate = if custom {
        v.f(3).clamp(1., 1000.)
    } else {
        p.frame_rate.as_f64()
    };
    let time = if v.popup(9) == 2 { p.comp_time } else { p.time };
    let ntsc = (rate * 1001. / 1000.).round();
    let drop = if custom {
        v.b(8)
    } else {
        (ntsc - rate).abs() > 0.01 && (ntsc * 1000. / 1001. - rate).abs() < 0.001
    };
    let nominal = rate.round().max(1.) as i64;
    let actual = if custom && drop && matches!(nominal, 30 | 60) {
        rate * 1000. / 1001.
    } else {
        rate
    };
    let n = if custom {
        (time.as_seconds() * actual + 1e-8).floor() as i64 + v.f(4).round() as i64
    } else {
        p.frame_rate.time_to_frame(time)
    };
    match v.popup(2) {
        2 => n.to_string(),
        3 | 4 => {
            let per_foot = if v.popup(2) == 3 { 16 } else { 40 };
            format!(
                "{}{}+{:02}",
                if n < 0 { "-" } else { "" },
                n.abs() / per_foot,
                n.abs() % per_foot
            )
        }
        _ => smpte(n, nominal, drop),
    }
}

pub(super) fn numbers(v: Params<'_>) -> String {
    let mut value = v.f(3);
    let rate = v.p.frame_rate;
    if v.b(2) {
        let mut hash =
            v.p.seed ^ (rate.time_to_frame(v.p.time) as u64).wrapping_mul(0x9e3779b97f4a7c15);
        hash = (hash ^ (hash >> 30)).wrapping_mul(0xbf58476d1ce4e5b9);
        hash = (hash ^ (hash >> 27)).wrapping_mul(0x94d049bb133111eb);
        value = (if value == 0. { 30000. } else { value }) * ((hash ^ (hash >> 31)) >> 11) as f64
            / (1u64 << 53) as f64;
    }
    let places = v.f(4).round().clamp(0., 9.) as usize;
    match v.popup(1) {
        2 => {
            let width = 5 + if places > 0 { 1 + places } else { 0 };
            format!(
                "{}{:0width$.places$}",
                if value < 0. { "-" } else { "" },
                value.abs()
            )
        }
        kind @ 3..=5 => {
            let fps = [30, 25, 24][(kind - 3) as usize];
            smpte(
                (v.p.time.as_seconds() * fps as f64).floor() as i64 + value.round() as i64,
                fps,
                false,
            )
        }
        6 => {
            let minutes = if v.b(5) {
                let now = Local::now();
                i64::from(now.hour() * 60 + now.minute())
            } else {
                value.floor() as i64
            };
            format!(
                "{:02}:{:02}",
                minutes.div_euclid(60).rem_euclid(24),
                minutes.rem_euclid(60)
            )
        }
        kind @ 7..=9 => {
            let epoch = if v.b(5) {
                Local::now().date_naive()
            } else {
                NaiveDate::from_ymd_opt(1995, 1, 1).unwrap()
            };
            let day = epoch + Duration::days(value.round().clamp(-30000., 30000.) as i64);
            match kind {
                7 => format!("{:02}/{:02}/{:04}", day.month(), day.day(), day.year()),
                8 => day.format("%b %d, %Y").to_string(),
                _ => day.format("%B %d, %Y").to_string(),
            }
        }
        10 => format!("{:X}", (value * 65536.).round() as i64 as u32),
        _ => format!("{value:.places$}"),
    }
}
#[test]
fn drop_frame_skips_two_labels_except_at_ten_minutes() {
    assert_eq!(smpte(1799, 30, true), "00:00:59;29");
    assert_eq!(smpte(1800, 30, true), "00:01:00;02");
    assert_eq!(smpte(17982, 30, true), "00:10:00;00");
}
