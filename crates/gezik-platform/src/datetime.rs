//! Dates in the file list: the system's short date and time on Windows, else
//! `YYYY-MM-DD HH:MM`, in local time.

use std::time::{SystemTime, UNIX_EPOCH};

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

    pub fn format(time: SystemTime) -> Option<String> {
        let ticks = time.duration_since(UNIX_EPOCH).ok()?.as_nanos() / 100 + UNIX_EPOCH_TICKS;
        let filetime = FILETIME { dwLowDateTime: ticks as u32, dwHighDateTime: (ticks >> 32) as u32 };
        let (mut utc, mut local) = (SYSTEMTIME::default(), SYSTEMTIME::default());
        let (mut date, mut clock) = ([0u16; 80], [0u16; 80]);
        // SAFETY: every pointer is to a live local of the right type; the buffers' lengths
        // are passed with them.
        unsafe {
            FileTimeToSystemTime(&filetime, &mut utc).ok()?;
            SystemTimeToTzSpecificLocalTime(None, &utc, &mut local).ok()?;
            let n = GetDateFormatEx(
                PCWSTR::null(),
                DATE_SHORTDATE,
                Some(&local as *const SYSTEMTIME),
                PCWSTR::null(),
                Some(&mut date),
                PCWSTR::null(),
            );
            let m = GetTimeFormatEx(
                PCWSTR::null(),
                TIME_NOSECONDS,
                Some(&local as *const SYSTEMTIME),
                PCWSTR::null(),
                Some(&mut clock),
            );
            if n <= 1 || m <= 1 {
                return None;
            }
            let date = String::from_utf16_lossy(&date[..n as usize - 1]);
            let clock = String::from_utf16_lossy(&clock[..m as usize - 1]);
            Some(format!("{date} {clock}"))
        }
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
