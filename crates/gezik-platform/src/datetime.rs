//! Dates in the file list: the system's short date and time on Windows, else
//! `YYYY-MM-DD HH:MM`, in local time; and the other formats of `[view] date-format` (spec 7.1).

use std::time::{SystemTime, UNIX_EPOCH};

use gezik_core::batch::date::DateParts;
use gezik_core::view::{DateFormat, relative_date};

/// `time` as `format` writes it; `now` is the time now (for `Relative`).
pub fn format_date(time: SystemTime, format: DateFormat, now: SystemTime) -> String {
    match format {
        DateFormat::System => format_datetime(time),
        DateFormat::Iso => local_date_parts(time).map(|p| iso_text(&p, true)).unwrap_or_default(),
        DateFormat::Short => short_date(time),
        DateFormat::Relative => {
            let age = match now.duration_since(time) {
                Ok(d) => i64::try_from(d.as_secs()).unwrap_or(i64::MAX),
                Err(e) => -i64::try_from(e.duration().as_secs()).unwrap_or(i64::MAX),
            };
            local_date_parts(now)
                .zip(local_date_parts(time))
                .and_then(|(now, then)| relative_date(&now, &then, age))
                .unwrap_or_else(|| format_datetime(time))
        }
    }
}

/// `YYYY-MM-DD`, and ` HH:MM` with `clock`.
fn iso_text(p: &DateParts, clock: bool) -> String {
    let date = format!("{:04}-{:02}-{:02}", p.year, p.month, p.day);
    if clock { format!("{date} {:02}:{:02}", p.hour, p.minute) } else { date }
}

/// The date only: the system's short date on Windows, `YYYY-MM-DD` elsewhere.
fn short_date(time: SystemTime) -> String {
    #[cfg(windows)]
    if let Some(text) = win::format_date(time) {
        return text;
    }
    local_date_parts(time).map(|p| iso_text(&p, false)).unwrap_or_default()
}

pub fn format_datetime(time: SystemTime) -> String {
    #[cfg(windows)]
    if let Some(text) = win::format(time) {
        return text;
    }
    let secs = match time.duration_since(UNIX_EPOCH) {
        Ok(d) => i64::try_from(d.as_secs()).unwrap_or(i64::MAX),
        Err(e) => -i64::try_from(e.duration().as_secs()).unwrap_or(i64::MAX),
    };
    iso_minutes(secs.saturating_add(local_offset(secs)))
}

/// `time` as a local date and time.
pub fn local_date_parts(time: SystemTime) -> Option<DateParts> {
    #[cfg(windows)]
    if let Some(parts) = win::parts(time) {
        return Some(parts);
    }
    let secs = match time.duration_since(UNIX_EPOCH) {
        Ok(d) => i64::try_from(d.as_secs()).ok()?,
        Err(e) => -i64::try_from(e.duration().as_secs()).ok()?,
    };
    let local = secs.saturating_add(local_offset(secs));
    let (days, rest) = (local.div_euclid(86_400), local.rem_euclid(86_400));
    let (year, month, day) = civil_from_days(days);
    Some(DateParts {
        year: i32::try_from(year).ok()?,
        month: month as u8,
        day: day as u8,
        hour: (rest / 3600) as u8,
        minute: (rest % 3600 / 60) as u8,
        second: (rest % 60) as u8,
    })
}

/// `YYYY-MM-DD HH:MM` for seconds since 1970-01-01 (already shifted to local time).
fn iso_minutes(secs: i64) -> String {
    let (days, rest) = (secs.div_euclid(86_400), secs.rem_euclid(86_400));
    let (y, m, d) = civil_from_days(days);
    format!("{y:04}-{m:02}-{d:02} {:02}:{:02}", rest / 3600, rest % 3600 / 60)
}

/// Days since 1970-01-01 → (year, month, day) in the proleptic Gregorian calendar
/// (Howard Hinnant's `civil_from_days`).
fn civil_from_days(days: i64) -> (i64, u32, u32) {
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = (if mp < 10 { mp + 3 } else { mp - 9 }) as u32;
    let y = yoe + era * 400;
    (if m <= 2 { y + 1 } else { y }, m, d)
}

/// Seconds to add to UTC for local time at `secs`.
#[cfg(unix)]
fn local_offset(secs: i64) -> i64 {
    let t = secs as libc::time_t;
    // SAFETY: `tm` is plain data; localtime_r only writes into it.
    let mut tm: libc::tm = unsafe { std::mem::zeroed() };
    if unsafe { libc::localtime_r(&t, &mut tm) }.is_null() {
        return 0;
    }
    tm.tm_gmtoff as i64
}

/// Windows formats with the system's own calls; this is only the fallback if they fail.
#[cfg(not(unix))]
fn local_offset(_secs: i64) -> i64 {
    0
}

#[cfg(windows)]
mod win {
    use std::time::{SystemTime, UNIX_EPOCH};

    use windows::Win32::Foundation::{FILETIME, SYSTEMTIME};
    use windows::Win32::Globalization::{DATE_SHORTDATE, GetDateFormatEx, GetTimeFormatEx, TIME_NOSECONDS};
    use windows::Win32::System::Time::{FileTimeToSystemTime, SystemTimeToTzSpecificLocalTime};
    use windows::core::PCWSTR;

    /// 100 ns ticks from 1601-01-01 (FILETIME's epoch) to 1970-01-01.
    const UNIX_EPOCH_TICKS: u128 = 116_444_736_000_000_000;

    /// `time` as a local `SYSTEMTIME`.
    fn local(time: SystemTime) -> Option<SYSTEMTIME> {
        let ticks = time.duration_since(UNIX_EPOCH).ok()?.as_nanos() / 100 + UNIX_EPOCH_TICKS;
        let filetime = FILETIME { dwLowDateTime: ticks as u32, dwHighDateTime: (ticks >> 32) as u32 };
        let (mut utc, mut local) = (SYSTEMTIME::default(), SYSTEMTIME::default());
        // SAFETY: pointers to live locals of the right types.
        unsafe {
            FileTimeToSystemTime(&filetime, &mut utc).ok()?;
            SystemTimeToTzSpecificLocalTime(None, &utc, &mut local).ok()?;
        }
        Some(local)
    }

    /// The system's short date of `local`.
    fn date_text(local: &SYSTEMTIME) -> Option<String> {
        let mut date = [0u16; 80];
        // SAFETY: the buffer's length is passed with it; `local` is a live SYSTEMTIME.
        let n = unsafe {
            GetDateFormatEx(
                PCWSTR::null(),
                DATE_SHORTDATE,
                Some(local as *const SYSTEMTIME),
                PCWSTR::null(),
                Some(&mut date),
                PCWSTR::null(),
            )
        };
        (n > 1).then(|| String::from_utf16_lossy(&date[..n as usize - 1]))
    }

    /// The system's time of `local`, without seconds.
    fn clock_text(local: &SYSTEMTIME) -> Option<String> {
        let mut clock = [0u16; 80];
        // SAFETY: as in `date_text`.
        let m = unsafe {
            GetTimeFormatEx(
                PCWSTR::null(),
                TIME_NOSECONDS,
                Some(local as *const SYSTEMTIME),
                PCWSTR::null(),
                Some(&mut clock),
            )
        };
        (m > 1).then(|| String::from_utf16_lossy(&clock[..m as usize - 1]))
    }

    pub fn parts(time: SystemTime) -> Option<gezik_core::batch::date::DateParts> {
        let local_time = local(time)?;
        Some(gezik_core::batch::date::DateParts {
            year: i32::from(local_time.wYear),
            month: local_time.wMonth as u8,
            day: local_time.wDay as u8,
            hour: local_time.wHour as u8,
            minute: local_time.wMinute as u8,
            second: local_time.wSecond as u8,
        })
    }

    pub fn format(time: SystemTime) -> Option<String> {
        let local_time = local(time)?;
        let date = date_text(&local_time)?;
        let clock = clock_text(&local_time)?;
        Some(format!("{date} {clock}"))
    }

    /// The system's short date of `time`, without the time.
    pub fn format_date(time: SystemTime) -> Option<String> {
        date_text(&local(time)?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[test]
    fn iso_dates_are_right_around_leap_days_and_before_1970() {
        assert_eq!(iso_minutes(0), "1970-01-01 00:00");
        assert_eq!(iso_minutes(951_782_400), "2000-02-29 00:00");
        assert_eq!(iso_minutes(1_700_000_000), "2023-11-14 22:13");
        assert_eq!(iso_minutes(-60), "1969-12-31 23:59");
    }

    #[test]
    fn formats_a_real_time() {
        let text = format_datetime(UNIX_EPOCH + Duration::from_secs(1_700_000_000));
        assert!(text.contains("23") && text.chars().any(|c| c.is_ascii_digit()), "{text}");
    }
}

#[cfg(test)]
mod parts_tests {
    use super::*;

    #[test]
    fn local_parts_are_a_valid_date_and_match_the_list_format() {
        let now = SystemTime::now();
        let parts = local_date_parts(now).unwrap();
        assert!((1..=12).contains(&parts.month) && (1..=31).contains(&parts.day) && parts.hour < 24);
        // Off Windows the list shows `YYYY-MM-DD HH:MM` from the same conversion.
        #[cfg(not(windows))]
        assert_eq!(
            format_datetime(now),
            format!("{:04}-{:02}-{:02} {:02}:{:02}", parts.year, parts.month, parts.day, parts.hour, parts.minute)
        );
    }

    #[test]
    fn every_date_format_has_its_shape() {
        use gezik_core::view::DateFormat;
        use std::time::Duration;
        let now = SystemTime::now();
        let p = local_date_parts(now).unwrap();
        let iso = format_date(now, DateFormat::Iso, now);
        assert_eq!(iso, format!("{:04}-{:02}-{:02} {:02}:{:02}", p.year, p.month, p.day, p.hour, p.minute));
        let short = format_date(now, DateFormat::Short, now);
        #[cfg(not(windows))]
        assert_eq!(short, iso[..10]);
        #[cfg(windows)]
        assert!(!short.is_empty() && !short.contains(':'), "the system's date, no time: {short}");
        let old = UNIX_EPOCH + Duration::from_secs(1_700_000_000);
        assert_eq!(format_date(old, DateFormat::System, now), format_datetime(old));
        assert_eq!(format_date(old, DateFormat::Relative, now), format_datetime(old), "long ago: the system's");
        assert_eq!(format_date(now - Duration::from_secs(300), DateFormat::Relative, now), "5 min ago");
        assert_eq!(format_date(now, DateFormat::Relative, now), "1 min ago");
        let later = now + Duration::from_secs(7200);
        assert_eq!(format_date(later, DateFormat::Relative, now), format_datetime(later), "the future: the system's");
        let earlier = now - Duration::from_secs(7200);
        let e = local_date_parts(earlier).unwrap();
        let day = if (e.year, e.month, e.day) == (p.year, p.month, p.day) { "Today" } else { "Yesterday" };
        assert_eq!(format_date(earlier, DateFormat::Relative, now), format!("{day} {:02}:{:02}", e.hour, e.minute));
    }
}
