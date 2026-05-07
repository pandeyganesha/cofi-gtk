// ─────────────────────────────────────────────────────────────────────────────
// layout.rs
//
// Grid + scatter-position computation, and the two font-size functions.
//
// Font-size philosophy
// ────────────────────
// Two distinct sizes are computed each frame:
//
//   base_size   — the size used at startup when *all* apps are shown.
//                 Derived from the 80th-percentile name length so the majority
//                 of names fill their cells nicely (a few very-long outliers
//                 may overflow their cell boundary, but they're very dim so it
//                 reads fine visually).
//
//   match_size  — the target size for apps that match the current query.
//                 Derived from the *longest matching name* in the conceptual
//                 grid those M apps would occupy if they were alone on screen.
//                 Grows smoothly as fewer names match (fewer → bigger cells →
//                 bigger font), approaching max_font_size when only one matches.
//
// Scatter positions
// ─────────────────
// calculate_grid() gives us a logical grid.  scatter_positions() adds a small
// deterministic jitter (±JITTER fraction of cell dimensions) so the layout
// looks organic rather than a rigid spreadsheet, while still guaranteeing
// every app stays within its cell's neighborhood and never overlaps a neighbor.
// ─────────────────────────────────────────────────────────────────────────────

use cairo::{Context, FontSlant, FontWeight, Format, ImageSurface};

// ── Grid ─────────────────────────────────────────────────────────────────────

/// Calculate (cols, rows) so that cols × rows ≥ n and the cells are as
/// square as possible given the screen aspect ratio.
pub fn calculate_grid(n: usize, screen_w: u32, screen_h: u32) -> (usize, usize) {
    if n <= 1 {
        return (1, 1);
    }

    let aspect = screen_w as f64 / screen_h as f64;
    let cols_f  = ((n as f64) * aspect).sqrt();

    let mut best_cols = 1usize;
    let mut best_rows = n;
    let mut best_waste = usize::MAX;

    for cols in [cols_f as usize, cols_f as usize + 1] {
        let cols = cols.max(1);
        let rows = (n + cols - 1) / cols;
        let waste = cols * rows - n;
        if waste < best_waste {
            best_waste = waste;
            best_cols  = cols;
            best_rows  = rows;
        }
    }

    (best_cols.max(1), best_rows.max(1))
}

// ── Scatter positions ─────────────────────────────────────────────────────────

/// Compute stable (cx, cy) pixel positions for `n` apps.
/// Uses the logical grid as a base, then adds a small deterministic jitter
/// so positions look organic while remaining within their cell.
pub fn scatter_positions(n: usize, screen_w: u32, screen_h: u32) -> Vec<(f64, f64)> {
    if n == 0 {
        return vec![];
    }

    let (cols, rows) = calculate_grid(n, screen_w, screen_h);
    let cell_w = screen_w as f64 / cols as f64;
    let cell_h = screen_h as f64 / rows as f64;

    // Jitter magnitude: ±JITTER fraction of cell dimension.
    // 0.20 gives enough "randomness" without any two names ever being
    // closer than ~60 % of a cell apart.
    const JITTER: f64 = 0.20;

    (0..n)
        .map(|i| {
            let col = i % cols;
            let row = i / cols;
            let cx = col as f64 * cell_w + cell_w / 2.0;
            let cy = row as f64 * cell_h + cell_h / 2.0;

            // Deterministic jitter: no RNG dependency, pure integer hashing.
            let jx = pseudo_rand(i.wrapping_mul(2)) * 2.0 - 1.0;      // −1…+1
            let jy = pseudo_rand(i.wrapping_mul(2).wrapping_add(1)) * 2.0 - 1.0;

            let x = (cx + jx * cell_w * JITTER)
                .clamp(cell_w * 0.15, screen_w as f64 - cell_w * 0.15);
            let y = (cy + jy * cell_h * JITTER)
                .clamp(cell_h * 0.15, screen_h as f64 - cell_h * 0.15);

            (x, y)
        })
        .collect()
}

/// Cheap, deterministic hash for jitter — no external crate needed.
fn pseudo_rand(seed: usize) -> f64 {
    let x = seed.wrapping_mul(2_654_435_761).wrapping_add(0x9e37_79b9);
    let x = x ^ (x >> 16);
    let x = x.wrapping_mul(0x45d9_f3b7);
    let x = x ^ (x >> 16);
    (x & 0xFFFF) as f64 / 65_535.0
}

// ── Font sizes ────────────────────────────────────────────────────────────────

/// Compute the base font size to use at startup when ALL apps are visible.
///
/// Uses the 80th-percentile name (by character count) as the reference, so
/// most names fill their cell nicely.  Very long outliers may slightly overflow
/// their cell but they're rendered very dimly, so it looks intentional.
pub fn compute_base_font_size(
    names:       &[&str],
    screen_w:    u32,
    screen_h:    u32,
    font_family: &str,
    min_size:    f64,
    max_size:    f64,
) -> f64 {
    let n = names.len();
    if n == 0 {
        return min_size;
    }

    let (cols, rows) = calculate_grid(n, screen_w, screen_h);
    let cell_w = screen_w as f64 / cols as f64;
    let cell_h = screen_h as f64 / rows as f64;

    // 80th-percentile name by length.  Sorting by char count lets us pick a
    // representative without being dragged down by one unusually long outlier.
    let mut lengths: Vec<usize> = names.iter().map(|s| s.chars().count()).collect();
    lengths.sort_unstable();
    let p80_len = lengths[((n as f64 * 0.80) as usize).min(n - 1)];

    // Find a name whose length is closest to p80_len.
    let reference = names
        .iter()
        .min_by_key(|s| s.chars().count().abs_diff(p80_len))
        .copied()
        .unwrap_or(names[0]);

    // Use 85 % of cell dimensions as the text budget.
    fit_text_in_box(reference, cell_w * 0.85, cell_h * 0.85, font_family, min_size, max_size)
}

/// Compute the target size for apps that *match* the current query.
///
/// Conceptually places the M matching apps in their own full-screen grid and
/// binary-searches for the largest font that fits the longest matching name.
/// The result is always ≥ `base_size` and ≤ `max_size`.
pub fn compute_match_font_size(
    matching_names: &[&str],
    screen_w:       u32,
    screen_h:       u32,
    font_family:    &str,
    base_size:      f64,
    max_size:       f64,
) -> f64 {
    let n = matching_names.len();
    if n == 0 {
        return base_size;
    }

    let (cols, rows) = calculate_grid(n, screen_w, screen_h);
    let cell_w = screen_w as f64 / cols as f64;
    let cell_h = screen_h as f64 / rows as f64;

    // Use the longest matching name so no matching app overflows.
    let longest = matching_names
        .iter()
        .max_by_key(|s| s.chars().count())
        .copied()
        .unwrap_or("");

    if longest.is_empty() {
        return base_size;
    }

    fit_text_in_box(longest, cell_w * 0.85, cell_h * 0.85, font_family, base_size, max_size)
}

// ── Internal helper ───────────────────────────────────────────────────────────

/// Binary-search for the largest font size where `text` fits in
/// `max_w × max_h` pixels using the given font face.
fn fit_text_in_box(
    text:        &str,
    max_w:       f64,
    max_h:       f64,
    font_family: &str,
    lo_bound:    f64,
    hi_bound:    f64,
) -> f64 {
    // Throw-away 1×1 surface just for text measurement.
    let dummy = ImageSurface::create(Format::ARgb32, 1, 1).expect("dummy surface");
    let cr    = Context::new(&dummy).expect("dummy context");
    cr.select_font_face(font_family, FontSlant::Normal, FontWeight::Normal);

    let mut lo   = lo_bound;
    let mut hi   = hi_bound;
    let mut best = lo_bound;

    // 24 iterations → sub-pixel precision.
    for _ in 0..24 {
        let mid = (lo + hi) / 2.0;
        cr.set_font_size(mid);
        let ext = cr.text_extents(text).unwrap();
        if ext.width() <= max_w && ext.height() <= max_h {
            best = mid;
            lo   = mid;
        } else {
            hi = mid;
        }
    }

    best.clamp(lo_bound, hi_bound)
}
