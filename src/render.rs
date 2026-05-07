// ─────────────────────────────────────────────────────────────────────────────
// render.rs
//
// Everything visual lives here.  One public function: draw_frame().
//
// Visual states of each app name
// ───────────────────────────────
//   SELECTED   — currently highlighted by arrow keys (in visible set).
//                Rendered in accent colour, slightly larger.
//
//   MATCHING   — matches the current query, not selected.
//                Bright white, at its current peak_size.
//
//   STICKY     — was matching at some point (peak_size > base_size) but no
//                longer matches the current (longer) query.  Size is frozen at
//                its peak; colour is a mid-dim so the user can see it used to
//                grow but is now stale.  NOT selectable.
//
//   DIM        — never matched yet (or query just started / cleared).
//                Very faint; size == base_size.
//
// All apps are drawn every frame — only colour and size differ.
// ─────────────────────────────────────────────────────────────────────────────

use cairo::{Context, FontSlant, FontWeight, ImageSurface};

use crate::{config::Config, desktop::DesktopEntry};

// ─────────────────────────────────────────────────────────────────────────────

/// Draw one complete frame.
///
/// `app_sizes`     — per-app font sizes (peak, never decreases while query active).
/// `app_positions` — per-app (cx, cy) centre positions in pixels.
/// `base_font_size`— the size used at startup / when all apps are shown.
#[allow(clippy::too_many_arguments)]
pub fn draw_frame(
    surface:        &ImageSurface,
    apps:           &[DesktopEntry],
    visible:        &[usize],   // indices into `apps` that match the query
    selected:       Option<usize>,
    query:          &str,
    app_sizes:      &[f64],
    app_positions:  &[(f64, f64)],
    base_font_size: f64,
    w:              u32,
    h:              u32,
    config:         &Config,
) {
    let cr = Context::new(surface).expect("cairo context");

    // ── Background ────────────────────────────────────────────────────────────
    let [r, g, b, a] = config.theme.bg;
    cr.set_source_rgba(r, g, b, a);
    cr.paint().unwrap();

    if apps.is_empty() || app_positions.is_empty() || app_sizes.is_empty() {
        draw_search_bar(&cr, query, w, h, config);
        return;
    }

    let filtering = !query.is_empty();

    // Fast O(1) membership check for the visible set.
    let visible_set: std::collections::HashSet<usize> = visible.iter().copied().collect();

    // ── Draw all apps ─────────────────────────────────────────────────────────
    cr.select_font_face(
        &config.theme.font_family,
        FontSlant::Normal,
        FontWeight::Normal,
    );

    // Use a small epsilon to decide whether this app has ever "grown".
    let eps = 0.5; // half a pixel — safe float comparison margin

    for idx in 0..apps.len() {
        let app      = &apps[idx];
        let is_sel   = selected == Some(idx);
        let matches  = visible_set.contains(&idx);
        let peak     = app_sizes.get(idx).copied().unwrap_or(base_font_size);
        let has_grown = peak > base_font_size + eps;

        // ── Classify visual state ─────────────────────────────────────────────
        //
        //  selected  → accent
        //  matching  → bright
        //  sticky    → mid-dim (was matching, still has grown size)
        //  dim       → very faint (never matched or query empty)

        let (r, g, b, a) = if is_sel && matches {
            let [r, g, b, a] = config.theme.highlight;
            (r, g, b, a)
        } else if matches && filtering {
            let [r, g, b, a] = config.theme.text_match;
            (r, g, b, a)
        } else if filtering && has_grown {
            // Sticky: frozen at peak but visually "stale".
            // About 40 % of full brightness — clearly different from both
            // bright matching and the fully-dim never-matched names.
            let [r, g, b, _] = config.theme.text_match;
            (r, g, b, 0.38)
        } else {
            // Dim: never matched (or no query).
            let [r, g, b, a] = config.theme.text_dim;
            (r, g, b, a)
        };
        cr.set_source_rgba(r, g, b, a);

        // ── Font size ─────────────────────────────────────────────────────────
        // Selected gets a small extra bump on top of its already-grown size.
        let size = if is_sel && matches {
            (peak * 1.12).min(config.theme.max_font_size)
        } else {
            peak
        };
        cr.set_font_size(size);

        // ── Position: centre the text on the scatter point ────────────────────
        let (cx, cy) = app_positions.get(idx).copied().unwrap_or((
            w as f64 / 2.0,
            h as f64 / 2.0,
        ));

        let ext = cr.text_extents(&app.name).unwrap();
        let tx  = cx - ext.x_bearing() - ext.width()  / 2.0;
        let ty  = cy - ext.y_bearing() - ext.height() / 2.0;

        cr.move_to(tx, ty);
        cr.show_text(&app.name).unwrap();
    }

    // ── Search bar ────────────────────────────────────────────────────────────
    draw_search_bar(&cr, query, w, h, config);
}

// ─────────────────────────────────────────────────────────────────────────────

fn draw_search_bar(cr: &Context, query: &str, w: u32, h: u32, config: &Config) {
    if query.is_empty() {
        return;
    }

    let bar_h: f64 = 52.0;
    let bar_y = h as f64 - bar_h;

    // Semi-transparent background strip.
    cr.set_source_rgba(0.0, 0.0, 0.0, 0.60);
    cr.rectangle(0.0, bar_y, w as f64, bar_h);
    cr.fill().unwrap();

    // Thin separator line.
    cr.set_source_rgba(1.0, 1.0, 1.0, 0.14);
    cr.set_line_width(1.0);
    cr.move_to(0.0, bar_y);
    cr.line_to(w as f64, bar_y);
    cr.stroke().unwrap();

    // Query text with a blinking-cursor underscore.
    let text = format!("> {query}_");

    cr.select_font_face(
        &config.theme.font_family,
        FontSlant::Normal,
        FontWeight::Normal,
    );
    cr.set_font_size(22.0);
    cr.set_source_rgba(0.9, 0.9, 0.9, 1.0);

    let ext = cr.text_extents(&text).unwrap();
    let tx  = (w as f64 - ext.width()) / 2.0 - ext.x_bearing();
    let ty  = bar_y + (bar_h - ext.height()) / 2.0 - ext.y_bearing();

    cr.move_to(tx, ty);
    cr.show_text(&text).unwrap();
}
