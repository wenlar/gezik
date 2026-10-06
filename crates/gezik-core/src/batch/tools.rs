//! The tools Gezik can download (7-Zip and ffmpeg; pdfium later): where each build is, its
//! size and SHA-256, and which programs are inside.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tool {
    SevenZip,
    /// ffmpeg with ffprobe beside it (`programs = ["ffmpeg", "ffprobe"]`, `.exe` on Windows).
    Ffmpeg,
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
    /// The archive the download is (`zip`, `tar.xz` or `7z`).
    pub kind: &'static str,
}

/// 7-Zip 26.03 from 7-zip.org, repackaged by `scripts/tools/prepare.ps1`, and ffmpeg 9.0.2
/// (gyan.dev, BtbN and Martin Riedl builds, one solid 7z per platform), repackaged by
/// `scripts/tools/prepare-ffmpeg.ps1` (their output, pasted).
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
    // Made from these ffmpeg 9.0.2 builds (addresses in scripts/tools/prepare-ffmpeg.ps1), sha256 size name:
    // 4705843ccaaf54257c16ad90f3e952ece33c17df964ecf7bfdbb0f49c7171077 35430500 ffmpeg-9.0.2-essentials_build.7z
    // 6dcd0626f9f6d6c7323565e57410e9f37ed14a85946ec9ea9caff294ca32f5c0 135510041 ffmpeg-n9.0.2-17-g2a571b6068-winarm64-gpl-9.0.zip
    // 68ee646831adaae2495618346f3bba94ff207ff83bbd34d643e7004730d66269 151017172 ffmpeg-n9.0.2-17-g2a571b6068-linux64-gpl-9.0.tar.xz
    // 91afcd7695d4bbbb9e0057aa10baaf9875ec4e1a97303f9059ec971336a2ab46 127445692 ffmpeg-n9.0.2-17-g2a571b6068-linuxarm64-gpl-9.0.tar.xz
    // c8ed4c4e6978a03c485edbfe4e0a5dc2380f8a30bba5150531b31b094492d924 28395699 ffmpeg-9.0.2-riedl-macos-arm64-ffmpeg.zip
    // fcbe839537485eaee7a7a8bc5cbc0f90d53617e80943e8a5b2e31cb851197ea6 28317701 ffmpeg-9.0.2-riedl-macos-arm64-ffprobe.zip
    // 7c6b4125b191cbf773832dc51f424cf2b6bb7da43007d1e066f95909e47cacd4 33816391 ffmpeg-9.0.2-riedl-macos-x64-ffmpeg.zip
    // 2322438ed2f6319a691291b247d09c69dcaa3a982460d1f269a7e1af335cfdfd 33719233 ffmpeg-9.0.2-riedl-macos-x64-ffprobe.zip
    // 8ceb4b9ee5adedde47b31e975c1d90c73ad27b6b165a1dcd80c7c545eb65b903 35147 ffmpeg-n9.0.2-COPYING.GPLv3
    ToolBuild {
        tool: Tool::Ffmpeg,
        platform: Platform::WindowsX64,
        version: "9.0.2",
        url: "https://github.com/wenlar/gezik-tools/releases/download/ffmpeg-9.0.2-1/ffmpeg-9.0.2-windows-x64.7z",
        size: 31594971,
        sha256: "5179320dc19e82ec4ae1bffa3ae97a7dad5a4a0ff99bf385a7144071979d9ca2",
        programs: &["ffmpeg.exe", "ffprobe.exe"],
        kind: "7z",
    },
    ToolBuild {
        tool: Tool::Ffmpeg,
        platform: Platform::WindowsArm64,
        version: "9.0.2",
        url: "https://github.com/wenlar/gezik-tools/releases/download/ffmpeg-9.0.2-1/ffmpeg-9.0.2-windows-arm64.7z",
        size: 31490863,
        sha256: "91057011e90ac1e672555376539c7bb4bf2b073ae1062cd09f39530fbe49c631",
        programs: &["ffmpeg.exe", "ffprobe.exe"],
        kind: "7z",
    },
    ToolBuild {
        tool: Tool::Ffmpeg,
        platform: Platform::MacArm64,
        version: "9.0.2",
        url: "https://github.com/wenlar/gezik-tools/releases/download/ffmpeg-9.0.2-1/ffmpeg-9.0.2-macos-arm64.7z",
        size: 21142760,
        sha256: "5a505ff0bcedccdd1cf53533ca0d2defd517c94ffbd0603e0839a37935167c38",
        programs: &["ffmpeg", "ffprobe"],
        kind: "7z",
    },
    ToolBuild {
        tool: Tool::Ffmpeg,
        platform: Platform::MacX64,
        version: "9.0.2",
        url: "https://github.com/wenlar/gezik-tools/releases/download/ffmpeg-9.0.2-1/ffmpeg-9.0.2-macos-x64.7z",
        size: 26971531,
        sha256: "7146b576239aeac9d2922654e12e73fb29370dd606f17f141b5eb6732be98bc9",
        programs: &["ffmpeg", "ffprobe"],
        kind: "7z",
    },
    ToolBuild {
        tool: Tool::Ffmpeg,
        platform: Platform::LinuxX64,
        version: "9.0.2",
        url: "https://github.com/wenlar/gezik-tools/releases/download/ffmpeg-9.0.2-1/ffmpeg-9.0.2-linux-x64.7z",
        size: 53607273,
        sha256: "680e2ed286335f92e64eff9274690b512b0f7b6182cc285d676457fdc146996f",
        programs: &["ffmpeg", "ffprobe"],
        kind: "7z",
    },
    ToolBuild {
        tool: Tool::Ffmpeg,
        platform: Platform::LinuxArm64,
        version: "9.0.2",
        url: "https://github.com/wenlar/gezik-tools/releases/download/ffmpeg-9.0.2-1/ffmpeg-9.0.2-linux-arm64.7z",
        size: 45627266,
        sha256: "0030da4c678416e373769a75b4b134343f060e378dd4b3d1bd9a30e502f05c95",
        programs: &["ffmpeg", "ffprobe"],
        kind: "7z",
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

    /// ffmpeg for every platform: one solid 7z from the `ffmpeg-<version>-<n>` release, with
    /// ffmpeg first and ffprobe beside it, and 9.0.2 or newer (older ones turn iPhone grid HEICs
    /// into one tile).
    #[test]
    fn every_ffmpeg_build_is_complete() {
        for platform in Platform::ALL {
            let build = build_for(Tool::Ffmpeg, platform).expect("ffmpeg for every platform");
            let release = format!("https://github.com/wenlar/gezik-tools/releases/download/ffmpeg-{}-", build.version);
            assert!(build.url.starts_with(&release), "{}", build.url);
            assert!(build.url.ends_with(".7z"), "{}", build.url);
            assert_eq!(build.kind, "7z");
            assert_eq!(build.sha256.len(), 64);
            assert!(build.sha256.chars().all(|c| c.is_ascii_hexdigit() && !c.is_ascii_uppercase()));
            assert!(build.size > 0);
            let exe = if matches!(platform, Platform::WindowsX64 | Platform::WindowsArm64) { ".exe" } else { "" };
            assert_eq!(build.programs, [format!("ffmpeg{exe}"), format!("ffprobe{exe}")]);
            let version: Vec<u32> = build.version.split('.').map(|part| part.parse().unwrap()).collect();
            assert!(version >= vec![9, 0, 2], "{}", build.version);
        }
    }

    #[test]
    fn this_platform_is_known() {
        assert!(Platform::current().is_some());
    }
}
