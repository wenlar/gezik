# Performance

## Checks (Windows)

```powershell
scripts/perf/measure.ps1 -Config <folder>   # startup time and idle memory
scripts/perf/stress.ps1    # 100,000-file folder: memory and scrolling CPU
scripts/perf/tabs.ps1      # memory with 1 vs 20 tabs
scripts/perf/grid.ps1      # grid view, 1000 pictures: memory after paging through them
scripts/perf/panes.ps1     # one pane, two panes, and after closing the second
scripts/perf/windows.ps1   # one, two and five windows, and after closing the extra ones
scripts/perf/hidden.ps1    # shown, in the tray, minimized (1280×800)
```
```sh
cargo test -p gezik-core --release -- --ignored sorting_100k   # sorting 100,000 names
```

Last measured (release build, Windows 11, 2560×1440 at 359 Hz, default window 900×600
unless noted). Memory is Task Manager's "Memory" column (private working set):

| Check | Result | Target |
|---|---|---|
| Startup to window | 32-38 ms | <= ~60 ms |
| Idle memory, 900×600 | 5.1 MB | <= 7 MB |
| Idle memory, 1280×800 | 5.5 MB | |
| 100,000-file folder, after load | 13.1 MB | <= ~18 MB |
| Sorting 100,000 names | 46-52 ms | <= 50 ms |
| Scrolling CPU (100,000 files, 60 wheel steps) | ~300 ms with `max-fps = 120` | ~480 ms |
| Grid, 1000 pictures, after paging through them | 22.9 MB | <= ~50 MB |
| 1 vs 20 tabs | +0.4 MB | <= +2 MB |
| Second pane: open / after closing it | +0.8 MB / +0.2-0.3 MB | no growth per open and close |
| Second window / five windows / after closing the extra ones | +1.2-1.3 MB / +5.1-5.4 MB / +0.5 MB | <= +0.3 MB after closing |
| In the tray or minimized, 1280×800 | 5.3-5.6 MB (shown: 5.5-5.8 MB) | |

On Windows the window is drawn line by line, a few rows at a time, without a frame buffer
for the whole window, so memory hardly grows with the window size and hiding the window
gives back little. On macOS and Linux the software renderer still keeps one frame of 4 bytes
per pixel, also while the window is hidden.
After the second pane or extra windows close, a few hundred KB of freed but fragmented heap
stay. Opening and closing the second pane again does not add to it; with five windows it
rose a little per cycle in earlier runs (`windows.ps1 -Cycles 6`), and `windows.ps1` reports
a FAIL against its 0.3 MB bound.

The numbers are the range of two or three runs of each script, each with a fresh config
folder in `%TEMP%` (give `measure.ps1` one with `-Config`; its `state.toml` may set
`[window]` `width` and `height`; without it Gezik uses your own config). Sorting 100,000
names sits at its 50 ms target (46-52 ms between runs): within the spread, with no headroom. `tabs.ps1 -Method SendKeys` was used, since the
default `PostMessage` does not check that the tabs opened.

---

[← Back to README](../README.md)
