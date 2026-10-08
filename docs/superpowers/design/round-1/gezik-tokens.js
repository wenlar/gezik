// Gezik token derivation: neutrals per direction + one accent hue -> every themed colour.
(function () {
  const rgb = h => { h = h.replace('#', ''); return [0, 2, 4].map(i => parseInt(h.slice(i, i + 2), 16)); };
  const hex = a => '#' + a.map(v => Math.round(Math.max(0, Math.min(255, v))).toString(16).padStart(2, '0')).join('');
  const mix = (a, b, t) => { const A = rgb(a), B = rgb(b); return hex(A.map((v, i) => v + (B[i] - v) * t)); };
  const alpha = (h, a) => h.slice(0, 7) + Math.round(a * 255).toString(16).padStart(2, '0');
  const flat = (c, bg) => c.length === 9 ? mix(bg, c.slice(0, 7), parseInt(c.slice(7), 16) / 255) : c;
  const lum = h => { const c = rgb(h).map(v => { v /= 255; return v <= 0.03928 ? v / 12.92 : Math.pow((v + 0.055) / 1.055, 2.4); }); return 0.2126 * c[0] + 0.7152 * c[1] + 0.0722 * c[2]; };
  const contrast = (a, b) => { const x = lum(a), y = lum(b); return (Math.max(x, y) + 0.05) / (Math.min(x, y) + 0.05); };

  const NEUTRAL = {
    a: { // Graphite: cool chrome around one raised sheet
      dark: { chrome: '#131417', background: '#1b1c20', surface: '#202227', raised: '#26282e', foreground: '#ecedf0', muted: '#9ca0aa', border: '#2e3036', strong: '#6c707a' },
      light: { chrome: '#e8e9ed', background: '#ffffff', surface: '#f4f5f7', raised: '#ffffff', foreground: '#16171a', muted: '#575b65', border: '#d9dbe1', strong: '#868b95' }
    },
    b: { // Ledger: one warm plane, structure from hairlines
      dark: { chrome: '#181715', background: '#181715', surface: '#1f1e1b', raised: '#26241f', foreground: '#efebe4', muted: '#a6a095', border: '#34312c', strong: '#726d63' },
      light: { chrome: '#f9f7f2', background: '#f9f7f2', surface: '#f0ece4', raised: '#ffffff', foreground: '#1c1a16', muted: '#5c574d', border: '#dcd7cc', strong: '#8c8679' }
    }
  };
  const ACCENTS = {
    a: { light: '#c2410c', dark: '#ff8f57', name: 'Amber' },
    b: { light: '#0b7a69', dark: '#41d1b6', name: 'Teal' },
    magenta: { light: '#b0158f', dark: '#f27ad6', name: 'Magenta' }
  };
  const STATUS = {
    light: { danger: '#b42318', success: '#1a7f37', warning: '#9a5b00' },
    dark: { danger: '#ff7b6b', success: '#5ad27e', warning: '#f0b44c' }
  };
  const ICONS = {
    light: { folder: '#c98a12', image: '#13866f', video: '#c23a55', audio: '#7d4cc4', archive: '#9a6631', document: '#2d6fc0', code: '#4f8a22', other: '#7b828c' },
    dark: { folder: '#e9b649', image: '#4fc7a8', video: '#ec7088', audio: '#b994ef', archive: '#d39f69', document: '#6aaef0', code: '#9bd165', other: '#a2a9b2' }
  };

  function derive(dir, theme, accentKey, density) {
    const n = NEUTRAL[dir][theme], dark = theme === 'dark';
    const acc = ACCENTS[accentKey === 'magenta' ? 'magenta' : dir][theme];
    const ink = dark ? '#ffffff' : '#000000';
    const af = contrast(acc, '#ffffff') >= contrast(acc, '#101010') ? '#ffffff' : mix(acc, '#000000', 0.86);
    const s = STATUS[theme], ic = ICONS[theme];
    const compact = density === 'compact';
    const c = {
      background: n.background, surface: n.surface, 'surface-raised': n.raised, chrome: n.chrome,
      foreground: n.foreground, 'foreground-muted': n.muted,
      border: n.border, 'border-strong': n.strong,
      accent: acc, 'accent-foreground': af,
      'accent-hover': mix(acc, ink, 0.12), 'accent-pressed': mix(acc, ink, 0.24),
      selection: mix(n.background, acc, dark ? 0.30 : 0.15), 'selection-foreground': n.foreground,
      'selection-foreground-muted': mix(n.foreground, n.muted, 0.35),
      'selection-inactive': mix(n.background, n.foreground, dark ? 0.11 : 0.08),
      hover: dark ? '#ffffff0f' : '#0000000a', pressed: dark ? '#ffffff1c' : '#00000014',
      'focus-ring': acc, marquee: alpha(acc, dark ? 0.2 : 0.14), 'drop-target': alpha(acc, dark ? 0.28 : 0.2),
      'tab-active': dir === 'a' ? n.background : n.background, 'tab-inactive': n.chrome,
      'input-background': dark ? mix(n.background, '#000000', 0.25) : n.raised,
      danger: s.danger, 'danger-background': mix(n.background, s.danger, dark ? 0.16 : 0.09),
      success: s.success, warning: s.warning,
      progress: acc, 'progress-paused': s.warning, 'progress-error': s.danger,
      shadow: dark ? '#00000070' : '#0000001f', overlay: dark ? '#0000008c' : '#14141859',
      scrollbar: alpha(n.foreground, 0.22), 'scrollbar-hover': alpha(n.foreground, 0.42),
      'icon-folder': ic.folder, 'icon-image': ic.image, 'icon-video': ic.video, 'icon-audio': ic.audio,
      'icon-archive': ic.archive, 'icon-document': ic.document, 'icon-code': ic.code, 'icon-other': ic.other
    };
    const fs = 13, row = compact ? Math.max(16, Math.floor(26 * 0.8)) : 26, sp = compact ? Math.max(2, Math.floor(6 * 0.8)) : 6;
    const radius = dir === 'a' ? 6 : 3;
    const m = {
      'font-family': '""', 'font-size': fs, 'row-height': row, 'icon-size': 16, radius, spacing: sp,
      // derived (not theme keys)
      _fsSmall: Math.round(fs * 0.85), _fsHeading: Math.round(fs * 1.15), _radiusSmall: Math.max(2, Math.round(radius / 2)),
      _tab: row + 10, _toolbar: row + 2 * sp + 4, _stroke: 1.5
    };
    return { c, m, dark, accentName: ACCENTS[accentKey === 'magenta' ? 'magenta' : dir].name };
  }

  const NEW = ['surface-raised', 'selection-foreground-muted', 'chrome', 'border-strong', 'accent-hover', 'accent-pressed', 'selection-inactive', 'pressed', 'tab-active', 'tab-inactive', 'input-background', 'danger-background', 'success', 'warning', 'shadow', 'overlay', 'scrollbar', 'scrollbar-hover'];
  const RULES = {
    selection: 'mix(background, accent, 15% light / 30% dark)',
    'selection-foreground-muted': 'mix(foreground, foreground-muted, 35%): secondary text on selected rows',
    'selection-inactive': 'mix(background, foreground, 8% / 11%)',
    'focus-ring': '= accent', progress: '= accent', marquee: 'accent @ 14% / 20%', 'drop-target': 'accent @ 20% / 28%',
    'accent-hover': 'mix(accent, ink, 12%)', 'accent-pressed': 'mix(accent, ink, 24%)',
    'accent-foreground': 'white or near-black, whichever contrasts more with accent',
    'danger-background': 'mix(background, danger, 9% / 16%)', 'progress-paused': '= warning', 'progress-error': '= danger',
    scrollbar: 'foreground @ 22%', 'scrollbar-hover': 'foreground @ 42%', 'tab-active': '= background', 'tab-inactive': '= chrome'
  };

  function contrastPairs(c) {
    const bg = c.background;
    const P = [
      ['foreground / background', c.foreground, bg, 4.5],
      ['foreground-muted / background', c['foreground-muted'], bg, 4.5],
      ['foreground-muted / surface', c['foreground-muted'], c.surface, 4.5],
      ['foreground-muted / chrome', c['foreground-muted'], c.chrome, 4.5],
      ['selection-foreground / selection', c['selection-foreground'], c.selection, 4.5],
      ['selection-foreground-muted / selection', c['selection-foreground-muted'], c.selection, 4.5],
      ['accent-foreground / accent', c['accent-foreground'], c.accent, 4.5],
      ['danger / background', c.danger, bg, 4.5],
      ['accent / background', c.accent, bg, 4.5],
      ['focus-ring / selection', c['focus-ring'], c.selection, 3],
      ['border-strong / input-background', c['border-strong'], c['input-background'], 3]
    ];
    return P.map(([n, f, b, min]) => { const r = contrast(flat(f, bg), flat(b, bg)); return { n, r: r.toFixed(2), ok: r >= min, min }; });
  }

  function toml(dir, theme) {
    const { c, m } = derive(dir, theme, dir, 'comfortable');
    const L = [`name = "${dir === 'a' ? 'Graphite' : 'Ledger'} ${theme === 'dark' ? 'Dark' : 'Light'}"`, '', '[colors]'];
    Object.keys(c).forEach(k => L.push(`${k} = "${c[k]}"`));
    L.push('', '[metrics]');
    ['font-family', 'font-size', 'row-height', 'icon-size', 'radius', 'spacing'].forEach(k => L.push(`${k} = ${m[k]}`));
    return L.join('\n');
  }

  window.GezikTokens = { derive, contrast, contrastPairs, toml, mix, alpha, NEW, RULES, NEUTRAL, ACCENTS };
})();
