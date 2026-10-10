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
selection (it can be translucent, e.g. `"#88c0d033"`). `inset` (0-16) is the gap around
the file list; the classic themes use 0. Older themes that set `folder-icon` or `file-icon`
keep working: they are still read as `icon-folder` and `icon-other`.

---

[← Back to README](../README.md)
