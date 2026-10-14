//! Set Dates (16k): which of a file's times change, and to what.

use std::time::SystemTime;

use crate::batch::date::DateParts;

/// A file's modified, created and last opened times; `None` for one that is not there (or,
/// as a change, one left as it is).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Times {
    pub modified: Option<SystemTime>,
    pub created: Option<SystemTime>,
    pub accessed: Option<SystemTime>,
}

impl Times {
    pub fn is_empty(&self) -> bool {
        *self == Times::default()
    }
}

/// The times the user chose to change.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Fields {
    pub modified: bool,
    pub created: bool,
    pub accessed: bool,
}

/// What the chosen times become.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Source {
    /// One time for all (a typed date and time, or now).
    At(SystemTime),
    /// The photo's date taken; `None` if it has none.
    Taken(Option<SystemTime>),
    /// Another file's times, each to its own kind (or undo's old times).
    From(Times),
}

/// What to write over `current`: only the chosen fields that change. `None` when the file has
/// no date to take (skipped and counted). Without `has_created` (Linux) created is never written.
pub fn resolve(source: &Source, fields: Fields, current: &Times, has_created: bool) -> Option<Times> {
    let from = match *source {
        Source::At(time) => Times { modified: Some(time), created: Some(time), accessed: Some(time) },
        Source::Taken(time) => {
            let time = time?;
            Times { modified: Some(time), created: Some(time), accessed: Some(time) }
        }
        Source::From(times) => times,
    };
    let pick = |on: bool, new: Option<SystemTime>, old: Option<SystemTime>| new.filter(|n| on && old != Some(*n));
    Some(Times {
        modified: pick(fields.modified, from.modified, current.modified),
        created: pick(fields.created && has_created, from.created, current.created),
        accessed: pick(fields.accessed, from.accessed, current.accessed),
    })
}

/// `2024-05-17` and `14:30` or `14:30:05` as a local date and time; `None` if either is not.
pub fn parse_local(date: &str, time: &str) -> Option<DateParts> {
    let nums = |text: &str, sep: char| -> Option<Vec<u32>> {
        text.trim().split(sep).map(|part| part.trim().parse().ok()).collect()
    };
    let (d, t) = (nums(date, '-')?, nums(time, ':')?);
    let (&[year, month, day], &[hour, minute, ref second @ ..]) = (d.as_slice(), t.as_slice()) else { return None };
    let second = match second {
        [] => 0,
        [s] => *s,
        _ => return None,
    };
    let days = match month {
        2 if year % 4 == 0 && (year % 100 != 0 || year % 400 == 0) => 29,
        2 => 28,
        4 | 6 | 9 | 11 => 30,
        1..=12 => 31,
        _ => return None,
    };
    if !(1..=days).contains(&day) || hour > 23 || minute > 59 || second > 59 || !(1601..=9999).contains(&year) {
        return None;
    }
    Some(DateParts {
        year: year as i32,
        month: month as u8,
        day: day as u8,
        hour: hour as u8,
        minute: minute as u8,
        second: second as u8,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    fn at(secs: u64) -> SystemTime {
        SystemTime::UNIX_EPOCH + Duration::from_secs(secs)
    }

    const ALL: Fields = Fields { modified: true, created: true, accessed: true };

    #[test]
    fn only_the_chosen_fields_that_change_are_written() {
        let current = Times { modified: Some(at(10)), created: Some(at(5)), accessed: Some(at(20)) };
        let fields = Fields { modified: true, created: false, accessed: true };
        assert_eq!(
            resolve(&Source::At(at(20)), fields, &current, true),
            Some(Times { modified: Some(at(20)), ..Times::default() }),
            "created not chosen, accessed already so"
        );
        assert_eq!(
            resolve(&Source::At(at(1)), ALL, &current, true),
            Some(Times { modified: Some(at(1)), created: Some(at(1)), accessed: Some(at(1)) })
        );
    }

    #[test]
    fn created_is_left_where_it_cannot_be_written() {
        let out = resolve(&Source::At(at(1)), ALL, &Times::default(), false).unwrap();
        assert_eq!(out, Times { modified: Some(at(1)), created: None, accessed: Some(at(1)) });
    }

    #[test]
    fn no_date_taken_is_a_skip_and_another_file_gives_each_its_own() {
        assert_eq!(resolve(&Source::Taken(None), ALL, &Times::default(), true), None);
        assert_eq!(
            resolve(
                &Source::Taken(Some(at(7))),
                Fields { modified: true, ..Fields::default() },
                &Times::default(),
                true
            ),
            Some(Times { modified: Some(at(7)), ..Times::default() })
        );
        // A source file from Linux has no created time: that one stays.
        let other = Times { modified: Some(at(3)), created: None, accessed: Some(at(4)) };
        let current = Times { modified: Some(at(9)), created: Some(at(9)), accessed: Some(at(9)) };
        assert_eq!(
            resolve(&Source::From(other), ALL, &current, true),
            Some(Times { modified: Some(at(3)), created: None, accessed: Some(at(4)) })
        );
        assert!(resolve(&Source::From(current), ALL, &current, true).unwrap().is_empty(), "nothing changes");
    }

    #[test]
    fn typed_dates_are_checked() {
        let parts = parse_local(" 2024-02-29 ", "14:30").unwrap();
        assert_eq!(
            (parts.year, parts.month, parts.day, parts.hour, parts.minute, parts.second),
            (2024, 2, 29, 14, 30, 0)
        );
        assert_eq!(parse_local("2024-05-17", "08:05:09").unwrap().second, 9);
        for (date, time) in [
            ("2023-02-29", "00:00"),
            ("2024-13-01", "00:00"),
            ("2024-04-31", "00:00"),
            ("2024-05-17", "24:00"),
            ("2024-05-17", "12"),
            ("2024-05-17", "1:2:3:4"),
            ("17.05.2024", "12:00"),
            ("", "12:00"),
        ] {
            assert!(parse_local(date, time).is_none(), "{date} {time}");
        }
    }
}
