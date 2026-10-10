# Performance

## Checks (Windows)

```powershell
scripts/perf/measure.ps1   # startup time and idle memory
scripts/perf/stress.ps1    # 100,000-file folder: memory and scrolling CPU
scripts/perf/tabs.ps1      # memory with 1 vs 20 tabs
scripts/perf/grid.ps1      # grid view, 1000 pictures: memory after paging through them
```
```sh
cargo test -p gezik-core --release -- --ignored sorting_100k   # sorting 100,000 names
```

Last measured (release build, Windows 11, 359 Hz display):

| Check | Result | Target |
|---|---|---|
| Startup to window | 24-35 ms | <= ~60 ms |
| Idle memory, default window (900×600) | 6.8 MB system icons, 6.3 MB `icons = "gezik"` | <= 7 MB (5.8 MB before the view work) |
| 100,000-file folder, after load | 17.2 MB | <= ~18 MB |
| Sorting 100,000 names | 39 ms | <= 50 ms |
| Scrolling CPU (100,000 files) | ~270 ms with `max-fps = 120` (~840 ms with `max-fps = 0`) | ~480 ms / 2.6 s |
| Grid, 1000 pictures, after paging through ~600 | 24 MB | <= ~50 MB |
| 1 vs 20 tabs | +0.1 MB | <= +2 MB |

Idle memory grows with the window: the software renderer keeps one frame of 4 bytes per
pixel, so a 1334×600 window measures 7.8 MB (system icons) instead of 6.8 MB. To measure
with a clean config, pass a folder: `scripts/perf/measure.ps1 -Config <folder>` (its
`settings.toml` may set `[view]` `icons = "gezik"`).

---

[← Back to README](../README.md)
