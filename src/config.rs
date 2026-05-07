// ─────────────────────────────────────────────────────────────────────────────
// config.rs
//
// Loads ~/.config/cofi/config.toml and provides typed structs for everything
// visual.  All fields have sensible defaults so an empty or missing config file
// still works fine.
//
// TOML example:
//   [theme]
//   bg            = [0.0, 0.0, 0.0, 0.88]   # RGBA, 0.0–1.0
//   text_dim      = [1.0, 1.0, 1.0, 0.18]
//   text_match    = [1.0, 1.0, 1.0, 0.95]
//   highlight     = [0.18, 0.60, 1.0, 1.0]
//   font_family   = "Sans"
//   max_font_size = 96.0
//   min_font_size = 7.0
// ─────────────────────────────────────────────────────────────────────────────

use serde::Deserialize;

/// RGBA colour — each channel is 0.0 (dark/transparent) … 1.0 (bright/opaque).
pub type Color = [f64; 4];

// ── Theme ─────────────────────────────────────────────────────────────────────

#[derive(Deserialize, Clone)]
#[serde(default)] // every missing key falls back to Default::default()
pub struct Theme {
    /// Background fill of the overlay.
    /// Keep alpha < 1.0 so Hyprland's blur can bleed through.
    pub bg: Color,

    /// All apps when nothing is typed yet — very dim so the screen looks like
    /// a faint cloud of names.
    pub text_dim: Color,

    /// Apps that match the current query — bright.
    pub text_match: Color,

    /// The one app that is currently selected with the arrow keys — accent colour.
    pub highlight: Color,

    /// Any font that Cairo / fontconfig can find on your system.
    /// Examples: "Sans", "Noto Sans", "JetBrains Mono", "Inter"
    pub font_family: String,

    /// Font size when only ONE app matches — the biggest it will ever get.
    pub max_font_size: f64,

    /// Font size when ALL apps are visible at once — the smallest it will be.
    pub min_font_size: f64,
}

impl Default for Theme {
    fn default() -> Self {
        Theme {
            bg:            [0.0,  0.0,  0.0,  0.88],
            text_dim:      [1.0,  1.0,  1.0,  0.18],
            text_match:    [1.0,  1.0,  1.0,  0.95],
            highlight:     [0.18, 0.60, 1.0,  1.0 ],
            font_family:   "Sans".to_string(),
            max_font_size: 96.0,
            min_font_size: 7.0,
        }
    }
}

// ── Top-level config ──────────────────────────────────────────────────────────

#[derive(Deserialize, Clone, Default)]
#[serde(default)]
pub struct Config {
    pub theme: Theme,
}

impl Config {
    /// Try to read `~/.config/cofi/config.toml`.
    /// On any error (missing file, bad syntax, …) we silently use defaults.
    pub fn load() -> Self {
        let home = std::env::var("HOME").unwrap_or_else(|_| "/root".to_string());
        let path = format!("{home}/.config/cofi/config.toml");

        match std::fs::read_to_string(&path) {
            Ok(text) => toml::from_str(&text).unwrap_or_else(|err| {
                eprintln!("[cofi] Warning: could not parse config at {path}: {err}");
                Config::default()
            }),
            Err(_) => Config::default(), // file doesn't exist yet — that's fine
        }
    }
}
