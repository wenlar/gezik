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

/// 7-Zip 26.03 from 7-zip.org, repackaged by `scripts/tools/prepare.ps1` (its output, pasted).
pub static MANIFEST: &[ToolBuild] = &[
    ToolBuild {
        tool: Tool::SevenZip,
        platform: Platform::WindowsX64,
        version: "26.03",
        url: "https://github.com/wenlar/gezik-tools/releases/download/7zip-26.03-1/7zip-26.03-windows-x64.zip",
        size: 1074227,
        sha256: "57b6fc249ad76072b0934d5bd441759d687f7e9cc96b1b04479044e2dbdac227",
        programs: &["7z.exe", "7z.dll"],
        kind: "zip",
    },
    ToolBuild {
        tool: Tool::SevenZip,
        platform: Platform::WindowsArm64,
        version: "26.03",
        url: "https://github.com/wenlar/gezik-tools/releases/download/7zip-26.03-1/7zip-26.03-windows-arm64.zip",
        size: 1000861,
        sha256: "29d8028e56ca2bd5b25e0fefb35c4054cbad4dc6d7992d81b641371fde5fafe0",
        programs: &["7z.exe", "7z.dll"],
        kind: "zip",
    },
    ToolBuild {
        tool: Tool::SevenZip,
        platform: Platform::MacArm64,
        version: "26.03",
        url: "https://github.com/wenlar/gezik-tools/releases/download/7zip-26.03-1/7zip-26.03-macos-arm64.tar.xz",
        size: 1881992,
        sha256: "5ce3970dbee357b96001c3708af2217fc1301fec0ec2ff9c6a1302bbd332f747",
        programs: &["7zz"],
        kind: "tar.xz",
    },
    ToolBuild {
        tool: Tool::SevenZip,
        platform: Platform::MacX64,
        version: "26.03",
        url: "https://github.com/wenlar/gezik-tools/releases/download/7zip-26.03-1/7zip-26.03-macos-x64.tar.xz",
        size: 1881992,
        sha256: "5ce3970dbee357b96001c3708af2217fc1301fec0ec2ff9c6a1302bbd332f747",
        programs: &["7zz"],
        kind: "tar.xz",
    },
    ToolBuild {
        tool: Tool::SevenZip,
        platform: Platform::LinuxX64,
        version: "26.03",
        url: "https://github.com/wenlar/gezik-tools/releases/download/7zip-26.03-1/7zip-26.03-linux-x64.tar.xz",
        size: 1038940,
        sha256: "f8d0f1b712bede58ffdcc9737f42906dcd873bdbf8f074021ab9fd1c014ed800",
        programs: &["7zz"],
        kind: "tar.xz",
    },
    ToolBuild {
        tool: Tool::SevenZip,
        platform: Platform::LinuxArm64,
        version: "26.03",
        url: "https://github.com/wenlar/gezik-tools/releases/download/7zip-26.03-1/7zip-26.03-linux-arm64.tar.xz",
        size: 883672,
        sha256: "bc61691e313a1bcb77f8525d35c21d0ff4a557be88c3b02e3ae99aa302c5cffd",
        programs: &["7zz"],
        kind: "tar.xz",
    },
];

pub fn build_for(tool: Tool, platform: Platform) -> Option<&'static ToolBuild> {
    MANIFEST.iter().find(|b| b.tool == tool && b.platform == platform)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
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
