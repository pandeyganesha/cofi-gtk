# cofi — a universal Linux app launcher

Covers the whole screen with a blurred transparent overlay.
All your apps are shown at once, randomly arranged.
Start typing — matched names grow bigger; everything else disappears.
Arrow keys to move, Enter to launch.

*Powered by GTK4, `cofi` works seamlessly on almost any Linux distribution and desktop environment (GNOME, KDE, Hyprland, etc.) on both Wayland and X11.*

---

## Requirements

You will need the Rust toolchain and GTK4 development libraries.

**Ubuntu / Debian / Pop!_OS:**
```bash
sudo apt update
sudo apt install rustc cargo libgtk-4-dev build-essential
```

**Arch Linux:**
```bash
sudo pacman -S rust gtk4 pkg-config
```

**Fedora:**
```bash
sudo dnf install rust cargo gtk4-devel
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

## Setup & Shortcuts

### Ubuntu / GNOME (Custom Shortcut)

To bind `cofi` to a shortcut like `Super+C` (or any other combo) in Ubuntu:
1. Open **Settings** and go to **Keyboard**.
2. Scroll down to **Keyboard Shortcuts** and click **View and Customize Shortcuts**.
3. Scroll to the bottom and select **Custom Shortcuts**.
4. Click the **+** (Add) button.
5. Fill in the details:
   - **Name:** Cofi Launcher
   - **Command:** `/usr/local/bin/cofi`
   - **Shortcut:** Click the "Set Shortcut" button and press `Super+C` (or `Super+Space`).
6. Click **Add**. Now, whenever you press that combo, the launcher will instantly open!

### Hyprland

Add the following to your `~/.config/hypr/hyprland.conf`:

```
# 1. Bind a key to open cofi
bind = SUPER, Space, exec, cofi

# 2. Enable blur on the transparent GTK window
windowrulev2 = blur, class:^(com\.github\.pandeyganesha\.cofi)$
windowrulev2 = ignorezero, class:^(com\.github\.pandeyganesha\.cofi)$
```

---

## Config file

Create `~/.config/cofi/config.toml` (all fields are optional):

```toml
[theme]
# Background — keep alpha low so the compositor blur bleeds through nicely
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
| `src/main.rs`     | GTK4 Application plumbing + event loop            |
