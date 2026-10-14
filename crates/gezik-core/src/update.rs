//! Automatic updates (2026-10-14 güncelleme spec): which published GitHub Release is newer
//! than the running Gezik on its channel, which of its files fits this install, and that file's
//! line in `SHA256SUMS.txt`. Pure: the request, the download and the install are the app's
//! (`gezik/src/update.rs`).

use std::cmp::Ordering;

use serde::Deserialize;

use crate::checksum_file::{self, Algo, FileKind};

/// A version as `v<version>` tags carry it: `major.minor.patch`, then `-` and dot-separated
/// pre-release parts (`0.1.0-rc.2`). `+build` is allowed and ignored in comparisons.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Version {
    numbers: [u64; 3],
    pre: Vec<String>,
    /// As written (without a leading `v`): what goes into the file names and the folder.
    text: String,
}

impl Version {
    /// `None` unless `text` is a version: digits only in the numbers, `[0-9A-Za-z-]` in the
    /// other parts, none of them empty (so the text is safe as a folder's name).
    pub fn parse(text: &str) -> Option<Version> {
        let (rest, build) = match text.split_once('+') {
            Some((rest, build)) => (rest, Some(build)),
            None => (text, None),
        };
        let (core, pre) = match rest.split_once('-') {
            Some((core, pre)) => (core, Some(pre)),
            None => (rest, None),
        };
        let mut numbers = [0; 3];
        let mut parts = core.split('.');
        for number in &mut numbers {
            let part = parts.next()?;
            if part.is_empty() || !part.bytes().all(|b| b.is_ascii_digit()) {
                return None;
            }
            *number = part.parse().ok()?;
        }
        let ident = |part: &str| !part.is_empty() && part.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-');
        let pre: Vec<String> = pre.map(|pre| pre.split('.').map(str::to_owned).collect()).unwrap_or_default();
        if parts.next().is_some() || !pre.iter().all(|p| ident(p)) || build.is_some_and(|b| !b.split('.').all(ident))
        {
            return None;
        }
        Some(Version { numbers, pre, text: text.to_owned() })
    }

    /// A pre-release (`0.1.0-rc.2`).
    pub fn is_pre(&self) -> bool {
        !self.pre.is_empty()
    }

    pub fn as_str(&self) -> &str {
        &self.text
    }
}

impl Ord for Version {
    /// Semver precedence: the numbers, then a release over its pre-releases, then the parts one
    /// by one (numeric ones by value and before words, words by ASCII), then more parts.
    fn cmp(&self, other: &Version) -> Ordering {
        let part = |a: &String, b: &String| match (a.parse::<u64>(), b.parse::<u64>()) {
            (Ok(a), Ok(b)) => a.cmp(&b),
            (Ok(_), Err(_)) => Ordering::Less,
            (Err(_), Ok(_)) => Ordering::Greater,
            (Err(_), Err(_)) => a.cmp(b),
        };
        self.numbers.cmp(&other.numbers).then_with(|| match (self.pre.is_empty(), other.pre.is_empty()) {
            (true, true) => Ordering::Equal,
            (true, false) => Ordering::Greater,
            (false, true) => Ordering::Less,
            (false, false) => self
                .pre
                .iter()
                .zip(&other.pre)
                .map(|(a, b)| part(a, b))
                .find(|o| o.is_ne())
                .unwrap_or_else(|| self.pre.len().cmp(&other.pre.len())),
        })
    }
}

impl PartialOrd for Version {
    fn partial_cmp(&self, other: &Version) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

/// `[update] channel`: which releases count.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Channel {
    /// Pre-releases too while the running Gezik is one, else only releases.
    #[default]
    Auto,
    Stable,
    Pre,
}

impl Channel {
    pub fn from_key(key: &str) -> Option<Channel> {
        match key {
            "auto" => Some(Channel::Auto),
            "stable" => Some(Channel::Stable),
            "pre" => Some(Channel::Pre),
            _ => None,
        }
    }
}

/// A release as GitHub's API lists it (the rest of its fields are ignored).
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct Release {
    pub tag_name: String,
    pub draft: bool,
    pub prerelease: bool,
    pub html_url: String,
    pub assets: Vec<Asset>,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct Asset {
    pub name: String,
    pub browser_download_url: String,
    pub size: u64,
}

impl Release {
    pub fn asset(&self, name: &str) -> Option<&Asset> {
        self.assets.iter().find(|a| a.name == name)
    }
}

/// `GET /repos/{owner}/{repo}/releases`' answer; `None` when it is not one.
pub fn parse_releases(json: &[u8]) -> Option<Vec<Release>> {
    serde_json::from_slice(json).ok()
}

/// The newest release on `channel` that is newer than `running`, with its version: drafts and
/// tags that are not `v` + a version are skipped; off the pre-release channel so is a release
/// marked as one or whose version has a `-` part.
pub fn newest<'a>(releases: &'a [Release], channel: Channel, running: &Version) -> Option<(Version, &'a Release)> {
    let pre = match channel {
        Channel::Auto => running.is_pre(),
        Channel::Stable => false,
        Channel::Pre => true,
    };
    releases
        .iter()
        .filter(|r| !r.draft)
        .filter_map(|r| Some((Version::parse(r.tag_name.strip_prefix('v')?)?, r)))
        .filter(|(v, r)| pre || (!r.prerelease && !v.is_pre()))
        .max_by(|a, b| a.0.cmp(&b.0))
        .filter(|(v, _)| v > running)
}

/// How this Gezik was installed: which release file replaces it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Package {
    Windows,
    /// The portable zip (the same `gezik.exe`).
    WindowsPortable,
    /// `Gezik.app`, as a `.app.tar.gz`.
    MacApp,
    AppImage,
    LinuxTar,
}

impl Package {
    /// Its file in release `version`'s assets.
    pub fn asset_name(self, version: &Version) -> String {
        let end = match self {
            Package::Windows => "windows-x64.zip",
            Package::WindowsPortable => "windows-x64-portable.zip",
            Package::MacApp => "macos-universal.app.tar.gz",
            Package::AppImage => "linux-x64.AppImage",
            Package::LinuxTar => "linux-x64.tar.gz",
        };
        format!("gezik-{}-{end}", version.as_str())
    }

    /// The name of what is swapped in the unpacked package (the AppImage is not unpacked).
    pub fn program(self) -> &'static str {
        match self {
            Package::Windows | Package::WindowsPortable => "gezik.exe",
            Package::MacApp => "Gezik.app",
            Package::AppImage | Package::LinuxTar => "gezik",
        }
    }
}

/// File `name`'s SHA-256 (lowercase hex) in a `SHA256SUMS.txt` (`sha256sum` lines).
pub fn sha256_of(sums: &[u8], name: &str) -> Option<String> {
    let parsed = checksum_file::parse(&checksum_file::decode(sums), FileKind::Sums);
    parsed.entries.into_iter().find(|e| e.path == name && e.algo == Algo::Sha256).map(|e| e.hex)
}

/// What `gezik --version` prints for `version`, the check that an unpacked program is the one
/// expected (and runs here).
pub fn version_line(version: &Version) -> String {
    format!("gezik {}", version.as_str())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn v(text: &str) -> Version {
        Version::parse(text).unwrap()
    }

    fn release(tag: &str, prerelease: bool) -> Release {
        Release {
            tag_name: tag.to_owned(),
            draft: false,
            prerelease,
            html_url: format!("https://github.com/wenlar/gezik/releases/tag/{tag}"),
            assets: Vec::new(),
        }
    }

    #[test]
    fn versions_order_as_semver_says() {
        let ordered = [
            "0.1.0-alpha",
            "0.1.0-rc.2",
            "0.1.0-rc.9",
            "0.1.0-rc.10",
            "0.1.0-rc.10.1",
            "0.1.0-rc.x",
            "0.1.0",
            "0.1.1-rc.1",
            "0.1.1",
            "0.2.0",
            "0.10.0",
            "1.0.0",
        ];
        for pair in ordered.windows(2) {
            assert!(v(pair[0]) < v(pair[1]), "{} < {}", pair[0], pair[1]);
        }
        assert_eq!(v("1.0.0+build.5").cmp(&v("1.0.0")), Ordering::Equal);
        assert_eq!(v("0.1.0-rc.2").as_str(), "0.1.0-rc.2");
        assert!(v("0.1.0-rc.2").is_pre() && !v("0.1.0").is_pre());
    }

    #[test]
    fn what_is_not_a_version_is_refused() {
        for bad in ["", "1", "1.2", "1.2.3.4", "a.b.c", "1.2.3-", "1.2.3-rc..1", "1.2.3-rc/../x", "1.2.3+", "-1.2.3", " 1.2.3"]
        {
            assert_eq!(Version::parse(bad), None, "{bad:?}");
        }
    }

    #[test]
    fn the_channel_decides_which_releases_count() {
        let releases =
            [release("v0.1.0-rc.3", true), release("v0.1.0", false), release("v0.2.0-rc.1", true), release("x", false)];
        let pick = |channel, running: &str| newest(&releases, channel, &v(running)).map(|(v, _)| v.text);
        assert_eq!(pick(Channel::Auto, "0.1.0-rc.2"), Some("0.2.0-rc.1".to_owned()), "an rc takes rcs");
        assert_eq!(pick(Channel::Auto, "0.0.9"), Some("0.1.0".to_owned()), "a release only releases");
        assert_eq!(pick(Channel::Stable, "0.1.0-rc.2"), Some("0.1.0".to_owned()));
        assert_eq!(pick(Channel::Pre, "0.1.0"), Some("0.2.0-rc.1".to_owned()));
        assert_eq!(pick(Channel::Auto, "0.1.0"), None, "nothing newer on the channel");
        assert_eq!(pick(Channel::Pre, "0.3.0"), None, "never older");
        assert_eq!(pick(Channel::Pre, "0.2.0-rc.1"), None, "not the same one");
    }

    #[test]
    fn drafts_marks_and_bad_tags_are_skipped() {
        let mut draft = release("v0.9.0", false);
        draft.draft = true;
        // Marked a pre-release though its version has no `-` part.
        let marked = release("v0.8.0", true);
        let releases = [draft, marked, release("0.7.0", false), release("v0.6.0-rc..1", false), release("v0.5.0", false)];
        let found = newest(&releases, Channel::Stable, &v("0.1.0")).map(|(v, _)| v.text);
        assert_eq!(found, Some("0.5.0".to_owned()));
        let found = newest(&releases, Channel::Pre, &v("0.1.0")).map(|(v, _)| v.text);
        assert_eq!(found, Some("0.8.0".to_owned()));
    }

    #[test]
    fn each_install_takes_its_own_file() {
        let version = v("0.2.0-rc.1");
        let names: Vec<String> = [
            Package::Windows,
            Package::WindowsPortable,
            Package::MacApp,
            Package::AppImage,
            Package::LinuxTar,
        ]
        .map(|p| p.asset_name(&version))
        .into();
        assert_eq!(
            names,
            [
                "gezik-0.2.0-rc.1-windows-x64.zip",
                "gezik-0.2.0-rc.1-windows-x64-portable.zip",
                "gezik-0.2.0-rc.1-macos-universal.app.tar.gz",
                "gezik-0.2.0-rc.1-linux-x64.AppImage",
                "gezik-0.2.0-rc.1-linux-x64.tar.gz",
            ]
        );
        assert_eq!(version_line(&version), "gezik 0.2.0-rc.1");
    }

    #[test]
    fn sha256sums_give_a_files_line() {
        let a = "48cca5616d11c40e96829e21751477c40fdf457f86ce4a113bfac441d416954b";
        let b = "E798E881B6087D1E719F8B06F2F10E7E28DF2863D8C28E2C7C63E64598A4F95D";
        let sums = format!("{a}  gezik-0.1.0-linux-x64.AppImage\r\n{b} *gezik-0.1.0-linux-x64.tar.gz\n");
        assert_eq!(sha256_of(sums.as_bytes(), "gezik-0.1.0-linux-x64.AppImage").as_deref(), Some(a));
        assert_eq!(sha256_of(sums.as_bytes(), "gezik-0.1.0-linux-x64.tar.gz"), Some(b.to_ascii_lowercase()));
        assert_eq!(sha256_of(sums.as_bytes(), "gezik-0.1.0-windows-x64.zip"), None);
        assert_eq!(sha256_of(b"not a sums file", "x"), None);
    }

    /// A real answer of `GET /repos/wenlar/gezik/releases` (2026-10-14, trimmed to two
    /// releases with fewer fields): parsed, then chosen from end to end.
    #[test]
    fn a_real_answer_is_read_and_chosen_from() {
        let releases = parse_releases(include_bytes!("../testdata/releases.json")).unwrap();
        assert_eq!(releases.len(), 2);
        assert!(releases.iter().all(|r| r.prerelease && !r.draft));
        let (version, release) = newest(&releases, Channel::Auto, &v("0.1.0-rc.1")).unwrap();
        assert_eq!(version.as_str(), "0.1.0-rc.2");
        assert_eq!(release.html_url, "https://github.com/wenlar/gezik/releases/tag/v0.1.0-rc.2");
        let asset = release.asset(&Package::Windows.asset_name(&version)).unwrap();
        assert_eq!(asset.size, 14133823);
        assert_eq!(
            asset.browser_download_url,
            "https://github.com/wenlar/gezik/releases/download/v0.1.0-rc.2/gezik-0.1.0-rc.2-windows-x64.zip"
        );
        assert!(release.asset("SHA256SUMS.txt").is_some());
        assert!(release.asset(&Package::MacApp.asset_name(&version)).is_none(), "rc.2 had only the dmg");
        assert!(newest(&releases, Channel::Stable, &v("0.1.0-rc.1")).is_none(), "no release yet");
        assert!(newest(&releases, Channel::Auto, &v("0.1.0-rc.2")).is_none());
        assert_eq!(parse_releases(b"{\"message\": \"API rate limit exceeded\"}"), None);
    }
}
