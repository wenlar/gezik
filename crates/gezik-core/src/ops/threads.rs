//! How many files a job copies or deletes at once.

/// What kind of disk a drive is; parallel work helps SSDs and hurts spinning disks.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DiskKind {
    Ssd,
    Hdd,
    Network,
    Unknown,
}

/// `copy-threads` in settings.toml.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum CopyThreads {
    #[default]
    Auto,
    /// 1–16.
    Fixed(u8),
}

/// Allowed `copy-threads` numbers.
pub const COPY_THREADS_RANGE: std::ops::RangeInclusive<u8> = 1..=16;

/// Workers for a job touching drives of `kinds`: a fixed setting wins; else the slowest kind
/// decides (SSD 6, network 4, spinning or unknown 1).
pub fn workers(threads: CopyThreads, kinds: &[DiskKind]) -> usize {
    match threads {
        CopyThreads::Fixed(n) => usize::from(n.clamp(*COPY_THREADS_RANGE.start(), *COPY_THREADS_RANGE.end())),
        CopyThreads::Auto => kinds
            .iter()
            .map(|kind| match kind {
                DiskKind::Ssd => 6,
                DiskKind::Network => 4,
                DiskKind::Hdd | DiskKind::Unknown => 1,
            })
            .min()
            .unwrap_or(1),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_slowest_disk_decides() {
        assert_eq!(workers(CopyThreads::Auto, &[DiskKind::Ssd]), 6);
        assert_eq!(workers(CopyThreads::Auto, &[DiskKind::Ssd, DiskKind::Network]), 4);
        assert_eq!(workers(CopyThreads::Auto, &[DiskKind::Ssd, DiskKind::Hdd]), 1);
        assert_eq!(workers(CopyThreads::Auto, &[DiskKind::Unknown]), 1);
        assert_eq!(workers(CopyThreads::Auto, &[]), 1);
    }

    #[test]
    fn a_fixed_number_wins_within_bounds() {
        assert_eq!(workers(CopyThreads::Fixed(3), &[DiskKind::Hdd]), 3);
        assert_eq!(workers(CopyThreads::Fixed(0), &[]), 1);
        assert_eq!(workers(CopyThreads::Fixed(99), &[]), 16);
    }
}
