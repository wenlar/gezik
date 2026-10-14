# Writing a theme

A theme only needs the values it changes; the rest comes from its `base` (`dark`, `light`,
`classic-dark`, `classic-light` or another theme's file name):

```toml
# themes/sunset.toml  ->  select it with  theme = "sunset"  in settings.toml
base = "dark"

[colors]
accent = "#ff8a3d"
icon-folder = "#ffb347"
```

See `themes/example.toml` for every color and size you can set. Colors you leave out are
worked out from the ones you set: an accent alone also recolors the selection, focus ring,
progress bars and so on. `icon-folder`, `icon-image`, `icon-video`, `icon-audio`,
`icon-archive`, `icon-document`, `icon-code` and `icon-other` color Gezik's own icons,
`focus-ring` is the keyboard focus in the file list and `marquee` is the rubber-band
selection (it can be translucent, e.g. `"#88c0d033"`). `git-modified`, `git-staged`, `git-untracked`,
`git-conflict` and `git-renamed` color the Git marks on rows in a repository. `inset` (0-16) is the gap around
the file list; the classic themes use 0. Older themes that set `folder-icon` or `file-icon`
keep working: they are still read as `icon-folder` and `icon-other`.

## Installing from GitHub

Settings ▸ Appearance ▸ Get Theme (or `Get Theme…` in the command palette): paste an address
and press Enter.

| Address | What is downloaded |
|---|---|
| `github.com/owner/repo` | the repository's default branch as a zip |
| `github.com/owner/repo/tree/<branch>/<folder>` | that branch; themes in `<folder>` |
| `github.com/owner/repo/blob/<branch>/x.toml` | that one file |
| any `https://…/x.toml` | that one file |

Each `.toml` at the top of the repository (or folder) or in its `themes/` folder becomes a
row; point at `Install` to preview the theme in every window, choose it to write
`themes/<name>.toml`. The first line of the file says where it came from:

```toml
# Installed from https://github.com/owner/repo (themes/nord.toml) on 2026-10-12
```

An installed theme's row has `Use Now`, `Check for Updates` (downloads the same address again
and shows whether it changed: `Replace` or `Keep Both`), `Open Source Page` and `Delete
Theme`. Nothing is checked on its own.

A theme is data only: colors, sizes, a font's name and its base. Gezik reads nothing else from
it, runs nothing and opens no other file. Only https is downloaded (redirects too), with the
system's own HTTP client; at most 5 MB, 64 KB per theme and 32 themes. The zip is read in
memory, never unpacked: only the themes you choose are written, under names Gezik picks
(`a-z`, `0-9`, `-`).

## Publishing a theme

Put one or more `.toml` files at the top of a public GitHub repository or in its `themes/`
folder and give the repository the topic `gezik-theme`. Set `name` for the title shown.

---

[← Back to README](../README.md)
