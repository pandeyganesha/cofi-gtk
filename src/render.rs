// ─────────────────────────────────────────────────────────────────────────────
// render.rs — everything visual lives here.
//
// Visual states per app name
// ──────────────────────────
//   SELECTED — highlighted by arrow keys (in visible set). Accent colour.
//   MATCHING — matches query, not selected. Bright white.
//   STICKY   — was matching but no longer (user typed more). Size frozen at
//              peak; mid-dim colour. NOT selectable.
//   DIM      — never matched / query cleared. Very faint; base size.
//
// All apps are drawn every frame — only colour and size differ.
//
// Layout pipeline per frame
// ─────────────────────────
//   1. Measure each inflated app's text box at its current peak size.
//   2. Run 5 passes of AABB separation on inflated apps: push overlapping
//      pairs apart along the axis of minimum penetration, then clamp to screen.
//   3. Draw all apps at their adjusted centres with per-pixel edge clamping.
// ─────────────────────────────────────────────────────────────────────────────

use cairo::{Context, FontSlant, FontWeight};
use crate::{config::Config, desktop::DesktopEntry};

// ─────────────────────────────────────────────────────────────────────────────

/// Gap between text boxes and from screen edges (pixels).
const MARGIN: f64 = 8.0;

/// Epsilon for "has this app ever grown above base size?"
const EPS: f64 = 0.5;

// ─────────────────────────────────────────────────────────────────────────────

#[allow(clippy::too_many_arguments)]
pub fn draw_frame(
    cr:             &Context,
    apps:           &[DesktopEntry],
    visible:        &[usize],
    selected:       Option<usize>,
    query:          &str,
    app_sizes:      &[f64],
    app_positions:  &[(f64, f64)],
    base_font_size: f64,
    w:              u32,
    h:              u32,
    config:         &Config,
) {
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

    // Select font face once — reused for all measurement and drawing below.
    cr.select_font_face(
        &config.theme.font_family,
        FontSlant::Normal,
        FontWeight::Normal,
    );

    // ── AABB separation pass for inflated apps ────────────────────────────────
    //
    // When two apps both grow (both match or are sticky) their enlarged text
    // boxes may overlap.  We push overlapping pairs apart along the axis of
    // minimum penetration, split equally, then re-clamp to screen bounds.
    // Dim apps (base size, ~18 % opacity) are excluded — harmless to overlap.

    let mut adj: Vec<(f64, f64)> = app_positions.to_vec();

    // Collect inflated apps: (app_idx, text_half_w, text_half_h)
    let inflated: Vec<(usize, f64, f64)> = (0..apps.len())
        .filter_map(|idx| {
            let peak = app_sizes.get(idx).copied().unwrap_or(base_font_size);
            if peak <= base_font_size + EPS {
                return None;
            }
            cr.set_font_size(peak);
            let ext = cr.text_extents(&apps[idx].name).unwrap();
            Some((idx, ext.width() / 2.0, ext.height() / 2.0))
        })
        .collect();

    // 5 passes — usually converges in 2-3 for typical cases.
    for _ in 0..5 {
        for i in 0..inflated.len() {
            for j in (i + 1)..inflated.len() {
                let (idx_i, hw_i, hh_i) = inflated[i];
                let (idx_j, hw_j, hh_j) = inflated[j];
                let (cx_i, cy_i) = adj[idx_i];
                let (cx_j, cy_j) = adj[idx_j];

                let dx = cx_j - cx_i;
                let dy = cy_j - cy_i;

                // Required centre-to-centre distance to clear the gap.
                let sep_x = hw_i + hw_j + MARGIN;
                let sep_y = hh_i + hh_j + MARGIN;

                let ov_x = sep_x - dx.abs();
                let ov_y = sep_y - dy.abs();

                if ov_x > 0.0 && ov_y > 0.0 {
                    // Push along the axis of minimum penetration depth.
                    let (push_x, push_y) = if ov_x <= ov_y {
                        let sign = if dx >= 0.0 { 1.0 } else { -1.0 };
                        (ov_x / 2.0 * sign, 0.0)
                    } else {
                        let sign = if dy >= 0.0 { 1.0 } else { -1.0 };
                        (0.0, ov_y / 2.0 * sign)
                    };
                    adj[idx_i] = (cx_i - push_x, cy_i - push_y);
                    adj[idx_j] = (cx_j + push_x, cy_j + push_y);
                }
            }
        }

        // Re-clamp centres to screen bounds after every pass.
        for &(idx, hw, hh) in &inflated {
            let (cx, cy) = adj[idx];
            adj[idx] = (
                cx.clamp(hw + MARGIN, w as f64 - hw - MARGIN),
                cy.clamp(hh + MARGIN, h as f64 - hh - MARGIN),
            );
        }
    }

    // ── Draw all apps ─────────────────────────────────────────────────────────
    for idx in 0..apps.len() {
        let app       = &apps[idx];
        let is_sel    = selected == Some(idx);
        let matches   = visible_set.contains(&idx);
        let peak      = app_sizes.get(idx).copied().unwrap_or(base_font_size);
        let has_grown = peak > base_font_size + EPS;

        // ── Colour ────────────────────────────────────────────────────────────
        let (r, g, b, a) = if is_sel && matches {
            let [r, g, b, a] = config.theme.highlight;
            (r, g, b, a)
        } else if matches && filtering {
            let [r, g, b, a] = config.theme.text_match;
            (r, g, b, a)
        } else if filtering && has_grown {
            // Sticky: ~40 % brightness — visually distinct from both bright
            // matching and fully-dim never-matched names.
            let [r, g, b, _] = config.theme.text_match;
            (r, g, b, 0.38)
        } else {
            let [r, g, b, a] = config.theme.text_dim;
            (r, g, b, a)
        };
        cr.set_source_rgba(r, g, b, a);

        // ── Font size ─────────────────────────────────────────────────────────
        let size = if is_sel && matches {
            (peak * 1.12).min(config.theme.max_font_size)
        } else {
            peak
        };
        cr.set_font_size(size);

        // ── Position ──────────────────────────────────────────────────────────
        // Use the separation-adjusted centre.  The tx/ty clamp below is a
        // pixel-perfect safety net (separation clamps centres, not baselines).
        let (cx, cy) = adj.get(idx).copied().unwrap_or((w as f64 / 2.0, h as f64 / 2.0));

        let ext = cr.text_extents(&app.name).unwrap();

        let tx_ideal = cx - ext.x_bearing() - ext.width()  / 2.0;
        let ty_ideal = cy - ext.y_bearing() - ext.height() / 2.0;

        let tx = tx_ideal
            .max(MARGIN - ext.x_bearing())
            .min(w as f64 - MARGIN - ext.x_bearing() - ext.width());

        let ty = ty_ideal
            .max(MARGIN - ext.y_bearing())
            .min(h as f64 - MARGIN - ext.y_bearing() - ext.height());

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

    cr.set_source_rgba(0.0, 0.0, 0.0, 0.60);
    cr.rectangle(0.0, bar_y, w as f64, bar_h);
    cr.fill().unwrap();

    cr.set_source_rgba(1.0, 1.0, 1.0, 0.14);
    cr.set_line_width(1.0);
    cr.move_to(0.0, bar_y);
    cr.line_to(w as f64, bar_y);
    cr.stroke().unwrap();

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
