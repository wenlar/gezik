//! The tools Gezik can download (7-Zip now; ffmpeg and pdfium later): where each build is,
//! its size and SHA-256, and which programs are inside.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tool {
    SevenZip,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Platform {
    WindowsX64,
    WindowsArm64,
    MacArm64,
    MacX64,
    LinuxX64,
    LinuxArm64,
}

impl Platform {
    pub const ALL: [Platform; 6] = [
        Platform::WindowsX64,
        Platform::WindowsArm64,
        Platform::MacArm64,
        Platform::MacX64,
        Platform::LinuxX64,
        Platform::LinuxArm64,
    ];

    pub fn current() -> Option<Platform> {
        Some(match (std::env::consts::OS, std::env::consts::ARCH) {
            ("windows", "x86_64") => Platform::WindowsX64,
            ("windows", "aarch64") => Platform::WindowsArm64,
            ("macos", "aarch64") => Platform::MacArm64,
            ("macos", "x86_64") => Platform::MacX64,
            ("linux", "x86_64") => Platform::LinuxX64,
            ("linux", "aarch64") => Platform::LinuxArm64,
            _ => return None,
        })
    }
}

pub struct ToolBuild {
    pub tool: Tool,
    pub platform: Platform,
    /// The tool's version, part of its folder name (`<data>/tools/7zip-25.01`).
    pub version: &'static str,
    pub url: &'static str,
    pub size: u64,
    pub sha256: &'static str,
    /// The programs inside the download (paths in the archive).
    pub programs: &'static [&'static str],
    /// The archive the download is (`zip` or `tar.xz`).
    pub kind: &'static str,
}

/// Filled by `scripts/tools/prepare.ps1` (Task 9); empty until then.
pub static MANIFEST: &[ToolBuild] = &[];

pub fn build_for(tool: Tool, platform: Platform) -> Option<&'static ToolBuild> {
    MANIFEST.iter().find(|b| b.tool == tool && b.platform == platform)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[ignore = "filled in by Task 9 (scripts/tools/prepare.ps1)"]
    fn every_seven_zip_build_is_complete() {
        for platform in Platform::ALL {
            let build = build_for(Tool::SevenZip, platform).expect("7-Zip for every platform");
            assert!(build.url.starts_with("https://github.com/wenlar/gezik-tools/releases/download/"), "{}", build.url);
            assert_eq!(build.sha256.len(), 64);
            assert!(build.sha256.chars().all(|c| c.is_ascii_hexdigit()));
            assert!(build.size > 0);
            assert!(!build.programs.is_empty());
        }
    }

    #[test]
    fn this_platform_is_known() {
        assert!(Platform::current().is_some());
    }
}
