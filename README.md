# cofi — a custom Hyprland app launcher

Covers the whole screen with a blurred transparent overlay.
All your apps are shown at once, randomly arranged.
Start typing — matched names grow bigger; everything else disappears.
Arrow keys to move, Enter to launch.

---

## Requirements

```
# All should already be on an Arch + Hyprland system:
sudo pacman -S rust cairo pkg-config
```

---

## Build & Install

```bash
cd cofi
cargo build --release

# Install to your PATH
sudo install -Dm755 target/release/cofi /usr/local/bin/cofi
```

---

## Hyprland config

### 1. Bind a key to open cofi

Add to `~/.config/hypr/hyprland.conf`:

```
bind = SUPER, Space, exec, cofi
```

### 2. Enable blur on the overlay (the frosted-glass effect)

```
layerrule = blur, cofi
layerrule = ignorezero, cofi
layerrule = ignorealpha 0.5, cofi
```

> **How blur works:** cofi draws a semi-transparent black background.
> Hyprland sees the transparency and blurs what is behind it.
> The `ignorezero` line prevents fully transparent pixels from being blurred
> (avoids a faint halo at the edges).

---

## Config file

Create `~/.config/cofi/config.toml` (all fields are optional):

```toml
[theme]
# Background — keep alpha low so the blur bleeds through nicely
bg            = [0.0, 0.0, 0.0, 0.88]

# App names when nothing is typed yet — very dim "cloud of names" look
text_dim      = [1.0, 1.0, 1.0, 0.18]

# App names that match what you're typing — bright
text_match    = [1.0, 1.0, 1.0, 0.95]

# The selected app (arrow key highlight) — accent colour
highlight     = [0.18, 0.60, 1.0, 1.0]

# Any font available via fontconfig on your system
font_family   = "Sans"

# Font size when only ONE app matches (maximum)
max_font_size = 96.0

# Font size when ALL apps are visible at once (minimum)
min_font_size = 7.0
```

---

## Usage

- **Type** to filter. Uses subsequence matching — "chr" matches "Chrome"
  and "character" but not "arch" (order matters).
- **Arrow keys** to navigate the grid.
- **Enter** to launch (only works after you type something).
- **Escape** to dismiss.

---

## How the matching works

A query is a *subsequence* of an app name when every character of the query
appears in the name **in order** (but not necessarily next to each other).

```
query "chr"  →  Chrome ✓  (c at 0, h at 1, r at 3)
query "chr"  →  character ✓  (c at 0, h at 2, r at 5)
query "chr"  →  arch  ✗  (c and h found, but no r after h)
```

---

## Tweaking the code

| File              | What it does                                      |
|-------------------|---------------------------------------------------|
| `src/config.rs`   | Theme struct + config file loading                |
| `src/desktop.rs`  | `.desktop` file parsing, app shuffling, launching |
| `src/lcs.rs`      | Subsequence matching algorithm                    |
| `src/layout.rs`   | Grid calculation + font size formula              |
| `src/nav.rs`      | Arrow-key navigation logic                        |
| `src/render.rs`   | Cairo drawing — change visuals here               |
| `src/main.rs`     | Wayland plumbing + event loop                     |
