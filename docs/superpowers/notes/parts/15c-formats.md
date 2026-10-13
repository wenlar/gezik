# 15c (formats): numbers, sizes, dates — notes

Spec §8.1, §8.2. The other half of 15c (status lines, prompts, errors in `t!`) is a separate branch.

## What is in

- **`gezik_i18n::number`**: `Number { decimal, group, group_from }`. Built-ins (CLDR): en and zh-Hans `1,234.5`; tr and de `1.234,5`; es `1234,5` but `12.345` (minimum grouping digits 2); fr `1 234,5` with U+202F. A dropped file names its own with `X-Gezik-Number: decimal=",", group="."` (a bad header is an error in the notices); without one it gets the built-in table for its code, else English. The pseudo language writes English numbers.
- **The language's number is one `AtomicU64`**, stored by `gezik_i18n::set` (on a language change or reload), read per row with `number()`: no lock, no allocation.
- **`{n}`** in `tn!` is grouped as the language does (`1,234 files`; English as `group_digits` wrote it before). `format::format` now writes arguments in place (`write!`), one allocation fewer per argument.
- **Sizes**: `format_size_in` keeps its signature and reads `number()`; `format_size_with` is the pure form. `1,5 MB` in Turkish. The integer part is never grouped (≤ 4 digits, as before). Units go through `t!("unit" => …)` (French `o Ko Mo` comes with 15f's translation).
- **`group_digits`** reads `number()`: every caller (preview line counts, tags, trash, field values, progress) follows the language without touching call sites.
- **Field values** (`fields::text`: exposure, aperture, focal length, frame rate, other reals) take the decimal separator; names built from fields (`name_text`) stay English so a rename template gives the same file name in every language.
- **Relative dates**: `tn!("{n} min ago" | "{n} min ago", …)`, `t!("Today {}", …)`, `t!("Yesterday {}", …)`; the clock goes in with `format_args!` (one allocation instead of two).
- **English is byte-identical**: the existing tests (`sizes_in_both_formats`, `formats_sizes`, `relative_dates_go_by_the_minute_the_day_and_yesterday`, `every_date_format_has_its_shape`) are unchanged and pass; per-language tests are in `number.rs`, `lib.rs` (`sizes_take_the_language_decimal_separator`), `fields.rs` and `check.rs`.

## Decisions against the task text

- **No month/day names**: no date format writes one today (`short` is the system's short date on Windows, `YYYY-MM-DD` elsewhere); spec §8.2 says none is added. Nothing to route through `t!`.
- **`system` dates stay as they are**: Windows already uses the OS region (`GetDateFormatEx`/`GetTimeFormatEx`, the user's locale, not the UI language); macOS and Linux keep `YYYY-MM-DD HH:MM` as spec §8.2 decides. If wanted later: Linux `setlocale(LC_TIME, "")` once + `strftime("%x %R")` (but `%x` is a 2-digit year in en_US), macOS one cached `NSDateFormatter` per thread (short date + short time styles).

## Follow-ups

- **Left for the messages half (call sites)**: `search.rs` "in {:.1} s", `size_map.rs` "{share:.0}%", GPS `{:.4}°` in `fields::text`, `group.rs` date headers ("Today", …), `DateFormat::label`/`SizeFormat::label` (menus, 15b).
- **`search::size_text`** stays English: the search box parses it back.
- **The pot changes here and in the messages branch**: whichever merges second re-runs `gezik-lang extract`.
