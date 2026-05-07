// ─────────────────────────────────────────────────────────────────────────────
// layout.rs
//
// Given N apps and the screen dimensions, calculate how many columns and rows
// the grid should have so that every cell is as close to square as possible
// and all N items fit on screen at once.
//
// Also computes the font size to use:
//   • When nothing is typed (all apps visible) → smallest size, apps fill screen.
//   • As the query filters apps down → font grows toward max_font_size.
//   • The growth is smooth: fewer matches → bigger text.
// ─────────────────────────────────────────────────────────────────────────────

/// Calculate (cols, rows) so that cols × rows ≥ n and the cells are as
/// square as possible given the screen aspect ratio.
pub fn calculate_grid(n: usize, screen_w: u32, screen_h: u32) -> (usize, usize) {
    if n == 0 {
        return (1, 1);
    }
    if n == 1 {
        return (1, 1);
    }

    let aspect = screen_w as f64 / screen_h as f64; // e.g. 1.78 for 16:9

    // We want cols / rows ≈ aspect and cols × rows ≥ n.
    // Starting point: cols ≈ sqrt(n × aspect).
    let cols_f = ((n as f64) * aspect).sqrt();

    // Try cols = floor and ceil, pick whichever gives squarer cells.
    let mut best_cols = 1usize;
    let mut best_rows = n;
    let mut best_waste = usize::MAX; // unused cells = cols*rows - n

    for cols in [cols_f as usize, cols_f as usize + 1] {
        let cols = cols.max(1);
        let rows = (n + cols - 1) / cols; // ceiling division
        let waste = cols * rows - n;
        if waste < best_waste {
            best_waste = waste;
            best_cols  = cols;
            best_rows  = rows;
        }
    }

    (best_cols.max(1), best_rows.max(1))
}

/// Compute the font size (in Cairo units ≈ pixels) to render app names at.
///
/// `n_visible` — how many apps are currently visible (matching the query).
/// `n_total`   — total number of apps loaded.
/// `screen_w/h` — screen dimensions.
/// `min_size`  — font size when all apps are shown (set in config).
/// `max_size`  — font size when only one app is shown (set in config).
pub fn compute_font_size(
    n_visible:  usize,
    n_total:    usize,
    screen_w:   u32,
    screen_h:   u32,
    min_size:   f64,
    max_size:   f64,
) -> f64 {
    if n_total == 0 || n_visible == 0 {
        return min_size;
    }

    // Base size: derived from the cell dimensions for the full grid.
    let (cols, rows) = calculate_grid(n_total, screen_w, screen_h);
    let cell_h = screen_h as f64 / rows as f64;

    // A cell's height gives us a natural cap — text shouldn't be taller than its row.
    let base = (cell_h * 0.40).clamp(min_size, max_size);

    if n_visible == n_total {
        // All apps visible → use base size.
        return base;
    }

    // Fewer matches → grow font smoothly toward max_size.
    // ratio = 1.0 when all visible, ratio → 0.0 when only 1 visible.
    let ratio = n_visible as f64 / n_total as f64;

    // Linear interpolation: ratio=1 → base,  ratio=0 → max_size.
    let size = base + (max_size - base) * (1.0 - ratio);

    size.clamp(base, max_size)
}
