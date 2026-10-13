# Gezik: rules for coding agents

- Commit messages and PR texts are in English.
- Never add `Co-Authored-By: Claude …` or any other AI attribution line to commits or PRs.
- Chat with the maintainer in Turkish.
- PRs must pass CI (fmt, clippy, tests on Windows, macOS, Linux).
- macOS test results go to `docs/superpowers/notes/macos-test-results.md` (see `macos-test.md` there for the build steps and checklist).
- New parts write notes to `docs/superpowers/notes/parts/<part>.md` and test items to `docs/superpowers/notes/checklists/<step>-macos.md` / `-linux.md`; never append to the shared followups or checklist files.
