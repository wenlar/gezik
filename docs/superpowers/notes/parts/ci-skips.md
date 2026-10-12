# Tests skipped or adjusted for CI

Tests that skip themselves when `CI` is set still run on a developer's machine; the skip prints
`skipped in CI: <reason>` on stderr.

## Skipped in CI

| Test | Platform | Why |
|---|---|---|
| `gezik-platform` `meta::windows::tests::explorers_tags_and_rating_are_read_and_the_file_is_left_alone` | Windows | The runner's service session gives no Explorer property store for a fresh JPEG (`None` where a desktop session gives no tags, rating 0). |

## Fixed in the CI setup instead of skipped

| Tests | Platform | Cause | Fix |
|---|---|---|---|
| Recycle Bin tests (`convert_tasks` ×2, `tasks::copy::a_test_leaves_nothing_in_the_real_trash`, `trash::a_killed_test_process_leaves_nothing_in_the_real_trash`), shortcut tests (`tasks::link::trashing_a_shortcut_leaves_its_folder`, `link::a_shortcut_points_at_its_target`), `terminal::powershell_lands_in_folders_with_quotes` | Windows | The runner's `TEMP` is an 8.3 short path (`C:\Users\RUNNER~1`); the Recycle Bin, shortcuts and PowerShell give the long path back, so comparisons with the test folder failed. | `ci.yml` sets `TEMP`/`TMP` to `runner.temp` (a long path). Follow-up: compare long paths in these tests, so a machine with a short `TEMP` passes too. |
| `process::tests::process_tree_is_killed` | Linux | The container's PID 1 (`tail -f /dev/null`) reaps no orphans; the killed shell's children stay as zombies. | `ci.yml` runs the container with `--init`. |

## Fixed in the code

- Linux trash across file systems (`/tmp` on another device than `$HOME`, a USB stick):
  Gezik wrote an absolute `Path=` into `$topdir/.Trash-<uid>`, which its own reader refuses there,
  so the item was missing from Gezik's trash list, undo and Put Back. It now writes the path
  relative to `$topdir` (freedesktop.org). This fixed `tasks::copy`, `tasks::delete` and
  `trash` tests on the Linux runner, where `$HOME` (`/github/home`) is a separate mount.
