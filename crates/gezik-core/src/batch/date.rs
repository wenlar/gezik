//! Dates in new names: a local date and time, and the strftime subset templates use.

/// A local date and time, already in the user's time zone.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct DateParts {
    pub year: i32,
    pub month: u8,
    pub day: u8,
    pub hour: u8,
    pub minute: u8,
    pub second: u8,
}

/// The letters after `%` a format may use.
const LETTERS: &str = "YymdHMSjbB%";

const MONTHS: [&str; 12] = [
    "January",
    "February",
    "March",
    "April",
    "May",
    "June",
    "July",
    "August",
    "September",
    "October",
    "November",
    "December",
];

/// Whether `format` uses only `%Y %y %m %d %H %M %S %j %b %B %%`; the error names the bad part.
pub fn check_format(format: &str) -> Result<(), String> {
    let mut chars = format.chars();
    while let Some(c) = chars.next() {
        if c != '%' {
            continue;
        }
        match chars.next() {
            Some(letter) if LETTERS.contains(letter) => {}
            Some(letter) => return Err(format!("unknown date part %{letter}")),
            None => return Err("a date format cannot end with %".to_owned()),
        }
    }
    Ok(())
}

fn day_of_year(date: &DateParts) -> u32 {
    const BEFORE: [u32; 12] = [0, 31, 59, 90, 120, 151, 181, 212, 243, 273, 304, 334];
    let leap = (date.year % 4 == 0 && date.year % 100 != 0) || date.year % 400 == 0;
    let month = usize::from(date.month.clamp(1, 12)) - 1;
    BEFORE[month] + u32::from(date.day) + u32::from(leap && month >= 2)
}

/// `date` written with `format` (checked with [`check_format`] first; an unknown part is
/// written as it is).
pub fn format(format: &str, date: &DateParts) -> String {
    let mut out = String::new();
    let mut chars = format.chars();
    while let Some(c) = chars.next() {
        if c != '%' {
            out.push(c);
            continue;
        }
        match chars.next() {
            Some('Y') => out.push_str(&format!("{:04}", date.year)),
            Some('y') => out.push_str(&format!("{:02}", date.year.rem_euclid(100))),
            Some('m') => out.push_str(&format!("{:02}", date.month)),
            Some('d') => out.push_str(&format!("{:02}", date.day)),
            Some('H') => out.push_str(&format!("{:02}", date.hour)),
            Some('M') => out.push_str(&format!("{:02}", date.minute)),
            Some('S') => out.push_str(&format!("{:02}", date.second)),
            Some('j') => out.push_str(&format!("{:03}", day_of_year(date))),
            Some('B') => out.push_str(MONTHS[usize::from(date.month.clamp(1, 12)) - 1]),
            Some('b') => out.push_str(&MONTHS[usize::from(date.month.clamp(1, 12)) - 1][..3]),
            Some('%') => out.push('%'),
            Some(other) => {
                out.push('%');
                out.push(other);
            }
            None => out.push('%'),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn date() -> DateParts {
        DateParts { year: 2024, month: 3, day: 9, hour: 7, minute: 5, second: 2 }
    }

    #[test]
    fn writes_each_part() {
        assert_eq!(format("%Y-%m-%d %H.%M.%S", &date()), "2024-03-09 07.05.02");
        assert_eq!(format("%y%j %b %B 100%%", &date()), "24069 Mar March 100%");
    }

    #[test]
    fn checks_formats() {
        assert!(check_format("%Y-%m-%d_%H%M").is_ok());
        assert_eq!(check_format("%Q").unwrap_err(), "unknown date part %Q");
        assert!(check_format("50%").is_err());
    }

    #[test]
    fn day_of_year_counts_leap_days() {
        assert_eq!(day_of_year(&DateParts { year: 2023, month: 3, day: 1, ..date() }), 60);
        assert_eq!(day_of_year(&DateParts { year: 2024, month: 3, day: 1, ..date() }), 61);
        assert_eq!(day_of_year(&DateParts { year: 1900, month: 3, day: 1, ..date() }), 60);
    }
}
