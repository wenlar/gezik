//! Saved folder syncs (`[[sync-profiles]]`, spec 11 §8.4): two folders, a mode, the compare's
//! options and filter. Written by the compare's "Save Profile…", run from the palette, where
//! they only compare (a sync always goes through its summary question).

use gezik_core::compare::{Mode, Options};

/// At most this many (menu ids 2260–2299 are kept for them, spec 11 §13.3).
pub const SYNC_PROFILES_MAX: usize = 40;
const KEYS: [&str; 9] =
    ["name", "left", "right", "mode", "subfolders", "content", "ignore-dates", "ignore-hour", "filter"];

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SyncProfile {
    pub name: String,
    /// As written: tokens like `{pictures}` allowed, `..` not.
    pub left: String,
    pub right: String,
    /// `mode`, `subfolders`, `content`, `ignore-dates`, `ignore-hour`; the show buttons are not
    /// a profile's.
    pub options: Options,
    /// The compare's filter (spec 11 §7.6); checked when it runs.
    pub filter: String,
}

/// One `[[sync-profiles]]` entry; the error is why it is left out.
pub fn parse(value: &toml::Value) -> Result<SyncProfile, String> {
    let table = value.as_table().ok_or_else(|| format!("expected a table, got {value}"))?;
    if let Some(key) = table.keys().find(|key| !KEYS.contains(&key.as_str())) {
        return Err(format!("unknown key \"{key}\""));
    }
    let text = |key: &str| table.get(key).and_then(|v| v.as_str()).map(str::trim).filter(|t| !t.is_empty());
    let name = text("name").ok_or("name is missing")?;
    let folder = |key: &str| -> Result<String, String> {
        let path = text(key).ok_or_else(|| format!("{key} is missing"))?;
        if crate::paths::has_parent_segment(path) {
            return Err(format!("{key}: \"{path}\" must not contain \"..\""));
        }
        Ok(path.to_owned())
    };
    let (left, right) = (folder("left")?, folder("right")?);
    let mut options = Options::default();
    if let Some(value) = table.get("mode") {
        options.mode = value
            .as_str()
            .and_then(Mode::from_key)
            .ok_or_else(|| format!("mode: expected two-way, left-to-right or mirror, got {value}"))?;
    }
    for (key, flag) in [
        ("subfolders", &mut options.subfolders),
        ("content", &mut options.content),
        ("ignore-dates", &mut options.ignore_dates),
        ("ignore-hour", &mut options.ignore_hour),
    ] {
        if let Some(value) = table.get(key) {
            *flag = value.as_bool().ok_or_else(|| format!("{key}: expected true or false, got {value}"))?;
        }
    }
    let filter = match table.get("filter") {
        None => String::new(),
        Some(value) => value.as_str().ok_or_else(|| format!("filter: expected text, got {value}"))?.to_owned(),
    };
    Ok(SyncProfile { name: name.to_owned(), left, right, options, filter })
}

/// `profile` as its table: every key, so the file says what it does.
pub fn to_toml(profile: &SyncProfile) -> toml::Table {
    let o = &profile.options;
    let mut table = toml::Table::new();
    for (key, value) in [("name", &profile.name), ("left", &profile.left), ("right", &profile.right)] {
        table.insert(key.into(), toml::Value::String(value.clone()));
    }
    table.insert("mode".into(), toml::Value::String(o.mode.key().to_owned()));
    for (key, on) in [
        ("subfolders", o.subfolders),
        ("content", o.content),
        ("ignore-dates", o.ignore_dates),
        ("ignore-hour", o.ignore_hour),
    ] {
        table.insert(key.into(), toml::Value::Boolean(on));
    }
    table.insert("filter".into(), toml::Value::String(profile.filter.clone()));
    table
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::settings::Settings;

    #[test]
    fn profiles_are_read_and_bad_ones_warned() {
        let text = r#"
[[sync-profiles]]
name = "Photos backup"
left = "{pictures}/2024"
right = "E:/Yedek/foto"
mode = "mirror"
content = true
filter = "*.jpg *.cr2 | thumbs\\"

[[sync-profiles]]
name = "Bad mode"
left = "/a"
right = "/b"
mode = "both"

[[sync-profiles]]
name = "Up"
left = "{home}/../x"
right = "/b"

[[sync-profiles]]
name = "photos BACKUP"
left = "/a"
right = "/b"

[[sync-profiles]]
name = "No right"
left = "/a"
"#;
        let mut warnings = Vec::new();
        let settings = Settings::parse("settings.toml", text, &mut warnings);
        assert_eq!(settings.sync_profiles.len(), 1);
        let p = &settings.sync_profiles[0];
        assert_eq!(
            (p.name.as_str(), p.left.as_str(), p.right.as_str()),
            ("Photos backup", "{pictures}/2024", "E:/Yedek/foto")
        );
        assert_eq!(p.options, Options { mode: Mode::Mirror, content: true, ..Options::default() });
        assert_eq!(p.filter, "*.jpg *.cr2 | thumbs\\");
        let said: Vec<String> = warnings.iter().map(|w| w.message.clone()).collect();
        assert_eq!(
            said,
            [
                "sync-profiles[2]: mode: expected two-way, left-to-right or mirror, got \"both\"",
                "sync-profiles[3]: left: \"{home}/../x\" must not contain \"..\"",
                "sync-profiles[4]: \"photos BACKUP\" is already used; this one is left out",
                "sync-profiles[5]: right is missing",
            ]
        );
    }

    #[test]
    fn a_profile_round_trips_and_hand_written_broken_ones_stay() {
        let profile = SyncProfile {
            name: "Docs".into(),
            left: "{documents}".into(),
            right: "/mnt/usb/docs".into(),
            options: Options { mode: Mode::LeftToRight, subfolders: false, ignore_hour: true, ..Options::default() },
            filter: "| .git\\".into(),
        };
        assert_eq!(parse(&toml::Value::Table(to_toml(&profile))), Ok(profile.clone()));
        let before = "# mine\n[[sync-profiles]]\nname = \"broken\"\nmode = 3\n";
        let after = crate::settings_edit::with_sync_profiles(before, std::slice::from_ref(&profile)).unwrap();
        let mut warnings = Vec::new();
        let settings = Settings::parse("settings.toml", &after, &mut warnings);
        assert_eq!(settings.sync_profiles, [profile]);
        assert_eq!(warnings.len(), 1, "the broken one is kept as written: {after}");
        assert!(after.contains("# mine"));
        let cleared = crate::settings_edit::with_sync_profiles(&after, &[]).unwrap();
        assert!(Settings::parse("settings.toml", &cleared, &mut Vec::new()).sync_profiles.is_empty());
    }
}
