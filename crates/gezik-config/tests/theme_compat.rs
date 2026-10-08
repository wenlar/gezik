//! Theme files written before the design pass still load, without warnings, and get the
//! new colors worked out from their own.

use gezik_config::Color;
use gezik_config::theme::{COLOR_KEYS, ResolvedTheme, resolve_theme};
use gezik_config::theme_rules::{contrast, flatten};
use std::collections::HashMap;

fn hex(text: &str) -> Color {
    Color::parse(text).unwrap()
}

fn load(id: &str, text: &str) -> ResolvedTheme {
    let themes = HashMap::from([(id.to_owned(), text.to_owned())]);
    let mut warnings = Vec::new();
    let theme = resolve_theme(id, &themes, &mut warnings).unwrap();
    assert!(warnings.is_empty(), "{id}: {warnings:?}");
    for key in COLOR_KEYS {
        assert_ne!(theme.colors.get(key), Color::default(), "{id}: {key} is empty");
    }
    theme
}

fn check(theme: &ResolvedTheme, expected: &[(&str, &str)]) {
    for (key, value) in expected {
        assert_eq!(theme.colors.get(key), hex(value), "{} {key}", theme.id);
    }
}

/// The 18 colors added by the design pass, as the rules make them for the old built-ins
/// loaded as user files (both extend the Graphite `dark`, so success and warning are its).
#[test]
fn old_full_theme_files_keep_their_values() {
    let light = load("old-light", include_str!("themes/old-light.toml"));
    check(
        &light,
        &[
            ("background", "#fafafa"),
            ("selection", "#cce4f7"),
            ("accent", "#005fb8"),
            ("hover", "#0000000d"),
            ("progress-paused", "#9d5d00"),
            ("chrome", "#e4e4e4"),
            ("surface-raised", "#fafafa"),
            ("input-background", "#fafafa"),
            ("border-strong", "#8b8b8b"),
            ("accent-hover", "#0054a2"),
            ("accent-pressed", "#00488c"),
            ("selection-foreground-muted", "#343434"),
            ("selection-inactive", "#e8e8e8"),
            ("pressed", "#00000014"),
            ("tab-active", "#fafafa"),
            ("tab-inactive", "#e4e4e4"),
            ("danger-background", "#f5e7e6"),
            ("success", "#5ad27e"),
            ("warning", "#f0b44c"),
            ("shadow", "#0000001f"),
            ("overlay", "#14141859"),
            ("scrollbar", "#1b1b1b38"),
            ("scrollbar-hover", "#1b1b1b6b"),
        ],
    );
    let dark = load("old-dark", include_str!("themes/old-dark.toml"));
    check(
        &dark,
        &[
            ("background", "#1c1c1c"),
            ("selection", "#2d4f6b"),
            ("accent-foreground", "#000000"),
            ("chrome", "#141414"),
            ("surface-raised", "#2a2a2a"),
            ("input-background", "#151515"),
            ("border-strong", "#898989"),
            ("accent-hover", "#73d3ff"),
            ("accent-pressed", "#86d9ff"),
            ("selection-foreground-muted", "#dedede"),
            ("selection-inactive", "#353535"),
            ("pressed", "#ffffff1c"),
            ("tab-active", "#1c1c1c"),
            ("tab-inactive", "#141414"),
            ("danger-background", "#402929"),
            ("shadow", "#00000070"),
            ("overlay", "#0000008c"),
            ("scrollbar", "#ffffff38"),
            ("scrollbar-hover", "#ffffff6b"),
        ],
    );
}

#[test]
fn nord_with_old_icon_names() {
    let nord = load("nord", include_str!("themes/nord.toml"));
    assert_eq!(nord.name, "Nord");
    assert_eq!(nord.metrics.font_family, "Inter");
    assert_eq!(nord.metrics.inset, 6.0);
    check(
        &nord,
        &[
            // Written: kept.
            ("accent-foreground", "#2e3440"),
            ("selection", "#434c5e"),
            ("hover", "#3b425280"),
            ("icon-folder", "#ebcb8b"),
            ("icon-other", "#9aa3ad"),
            // Worked out from what it wrote.
            ("focus-ring", "#88c0d0"),
            ("progress", "#88c0d0"),
            ("marquee", "#88c0d033"),
            ("drop-target", "#88c0d047"),
            ("chrome", "#20242d"),
            ("tab-inactive", "#20242d"),
            ("tab-active", "#2e3440"),
            ("surface-raised", "#393f4b"),
            ("input-background", "#232730"),
            ("border-strong", "#878d9a"),
            ("selection-foreground-muted", "#cfd4de"),
            ("selection-inactive", "#434954"),
            ("danger-background", "#453b47"),
            ("progress-error", "#bf616a"),
            // Not made from anything it wrote: the built-in dark's.
            ("progress-paused", "#f0b44c"),
            ("icon-image", "#4fc7a8"),
        ],
    );
}

#[test]
fn the_old_example_theme() {
    let example = load("example", include_str!("themes/example-old.toml"));
    assert_eq!(example.name, "Example (Nord)");
    check(
        &example,
        &[
            ("progress-paused", "#ebcb8b"),
            ("focus-ring", "#88c0d0"),
            ("chrome", "#20242d"),
            ("input-background", "#232730"),
            ("selection-inactive", "#434954"),
        ],
    );
}

#[test]
fn the_new_example_theme() {
    let example = load("example", include_str!("../templates/example.toml"));
    check(&example, &[("chrome", "#20242d"), ("surface-raised", "#393f4b")]);
    assert_eq!(example.metrics.inset, 6.0);
}

#[test]
fn accent_only_and_dark_from_light() {
    let accent = load("accent-only", include_str!("themes/accent-only.toml"));
    check(&accent, &[("selection", "#f3dcee"), ("focus-ring", "#b0158f"), ("chrome", "#e8e9ed")]);
    let night = load("light-night", include_str!("themes/light-night.toml"));
    check(&night, &[("hover", "#ffffff0f"), ("chrome", "#161616"), ("selection", "#512a1a")]);
}

/// §10.1: text ≥ 4.5, other marks ≥ 3, in every built-in theme and with each accent alone.
#[test]
fn contrast_holds_in_every_theme_and_accent() {
    let mut themes: Vec<ResolvedTheme> = ["light", "dark", "classic-light", "classic-dark"]
        .iter()
        .map(|id| resolve_theme(id, &HashMap::new(), &mut Vec::new()).unwrap())
        .collect();
    for (name, light, dark) in [
        ("amber", "#c2410c", "#ff8f57"),
        ("teal", "#0b7a69", "#41d1b6"),
        ("magenta", "#b0158f", "#f27ad6"),
        ("blue", "#005fb8", "#60cdff"),
    ] {
        themes.push(load(&format!("{name}-l"), &format!("base = \"light\"\n[colors]\naccent = \"{light}\"\n")));
        themes.push(load(&format!("{name}-d"), &format!("base = \"dark\"\n[colors]\naccent = \"{dark}\"\n")));
    }
    for t in &themes {
        let c = &t.colors;
        let pairs = [
            ("foreground/background", c.foreground, c.background, 4.5),
            ("foreground-muted/background", c.foreground_muted, c.background, 4.5),
            ("foreground-muted/surface", c.foreground_muted, c.surface, 4.5),
            ("foreground-muted/chrome", c.foreground_muted, c.chrome, 4.5),
            ("foreground-muted/input-background", c.foreground_muted, c.input_background, 4.5),
            ("selection-foreground/selection", c.selection_foreground, c.selection, 4.5),
            ("selection-foreground-muted/selection", c.selection_foreground_muted, c.selection, 4.5),
            ("foreground/selection-inactive", c.foreground, c.selection_inactive, 4.5),
            ("accent-foreground/accent", c.accent_foreground, c.accent, 4.5),
            ("danger/background", c.danger, c.background, 4.5),
            ("focus-ring/selection", c.focus_ring, c.selection, 3.0),
            ("focus-ring/background", c.focus_ring, c.background, 3.0),
            ("focus-ring/chrome", c.focus_ring, c.chrome, 3.0),
            ("border-strong/input-background", c.border_strong, c.input_background, 3.0),
        ];
        for (name, a, b, min) in pairs {
            let ratio = contrast(flatten(a, c.background), flatten(b, c.background));
            assert!(ratio >= min, "{}: {name} is {ratio:.2}, needs {min}", t.id);
        }
    }
}
