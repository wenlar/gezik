# Linux Docker GUI run for steps 9–14 (stopped, 2026-10-13)

Stopped by request after ~2 hours, before the results were written up. Saved here so the work can be resumed.

- What exists: `scripts/linux/gui-steps.sh` (new, 441 lines: GUI checks for steps 9–14 on X11 and Wayland), small fixes in `gui.sh`, `test.sh` and `examples/vpointer.rs`.
- Container `gezik-linux-run` (stopped, not removed) has the debug build; restart it with `docker start gezik-linux-run`.
- Screenshots: 158 files under `.superpowers/linux-shots/` (not in git), e.g. preview of JPG, MP3, MKV and PDF on X11.
- Last known state: the runner was looking at three unexpected failures; which ones was not recorded.
- Unit tests on Linux now run in CI (PR #114) on every pull request; this run only adds real-window GUI checks.

To resume: start the container, run `scripts/linux/gui-steps.sh` for X11 and Wayland, and write the results table here.
