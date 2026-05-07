// ─────────────────────────────────────────────────────────────────────────────
// render.rs
//
// Everything visual lives here.  One public function: draw_frame().
// It paints a single frame onto a cairo::ImageSurface.
//
// Layout recap:
//   • The screen is divided into cols × rows equally-sized cells.
//   • Each app's name is drawn centred inside its cell.
//   • In the "all apps" view (no query) every name is dim.
//   • When filtering, matching names grow brighter and bigger; non-matches
//     are simply not drawn.
//   • The selected app is drawn in the accent colour and slightly larger.
//   • A search bar appears at the bottom showing the current query.
// ─────────────────────────────────────────────────────────────────────────────

use cairo::{Context, FontSlant, FontWeight, ImageSurface};

use crate::{config::Config, desktop::DesktopEntry};

// ─────────────────────────────────────────────────────────────────────────────

/// Draw one complete frame.
///
/// `surface`  — Cairo surface to paint onto (ARgb32 format).
/// `apps`     — all loaded desktop entries (in their shuffled order).
/// `visible`  — indices into `apps` that match the current query.
///              Empty means "no query typed yet → show all apps dimly".
/// `selected` — index into `apps` of the highlighted item, or None.
/// `query`    — the text the user has typed so far.
/// `cols/rows`— grid dimensions (computed by layout::calculate_grid).
/// `w / h`    — screen pixel dimensions.
/// `config`   — theme settings.
pub fn draw_frame(
    surface:  &ImageSurface,
    apps:     &[DesktopEntry],
    visible:  &[usize],
    selected: Option<usize>,
    query:    &str,
    cols:     usize,
    rows:     usize,
    w:        u32,
    h:        u32,
    config:   &Config,
) {
    let cr = Context::new(surface).expect("cairo context");

    // ── Background ────────────────────────────────────────────────────────────
    let [r, g, b, a] = config.theme.bg;
    cr.set_source_rgba(r, g, b, a);
    cr.paint().unwrap();

    if apps.is_empty() || cols == 0 || rows == 0 {
        draw_search_bar(&cr, query, w, h, config);
        return;
    }

    let cell_w = w as f64 / cols as f64;
    let cell_h = h as f64 / rows as f64;

    // Decide which font size to use this frame.
    // We pass `n_visible` so the font grows as fewer apps remain.
    let filtering  = !query.is_empty();
    let n_visible  = if filtering { visible.len() } else { apps.len() };

    let font_size = crate::layout::compute_font_size(
        n_visible,
        apps.len(),
        w, h,
        config.theme.min_font_size,
        config.theme.max_font_size,
    );

    // Build a fast lookup set so we can check visibility in O(1).
    // (Vec of usize is small enough that a sorted vec + binary search would also
    //  work, but HashSet is clearest.)
    let visible_set: std::collections::HashSet<usize> = visible.iter().copied().collect();

    // ── Draw each app name ────────────────────────────────────────────────────
    cr.select_font_face(
        &config.theme.font_family,
        FontSlant::Normal,
        FontWeight::Normal,
    );

    for (idx, app) in apps.iter().enumerate() {
        // In filtering mode, skip non-matching apps entirely (invisible).
        if filtering && !visible_set.contains(&idx) {
            continue;
        }

        let is_selected = selected == Some(idx);

        // ── Choose colour ─────────────────────────────────────────────────────
        let [r, g, b, a] = if is_selected {
            config.theme.highlight
        } else if filtering {
            config.theme.text_match
        } else {
            config.theme.text_dim
        };
        cr.set_source_rgba(r, g, b, a);

        // ── Choose size ───────────────────────────────────────────────────────
        // Selected item is a bit larger for emphasis; capped at max_font_size.
        let size = if is_selected {
            (font_size * 1.25).min(config.theme.max_font_size)
        } else {
            font_size
        };
        cr.set_font_size(size);

        // ── Cell centre ───────────────────────────────────────────────────────
        let col = idx % cols;
        let row = idx / cols;
        let cx  = col as f64 * cell_w + cell_w / 2.0;
        let cy  = row as f64 * cell_h + cell_h / 2.0;

        // ── Centre the text glyph on (cx, cy) ─────────────────────────────────
        //
        // Cairo draws text from the *baseline* at the move_to point.
        // TextExtents gives us the bounding box relative to the baseline origin:
        //   x_bearing — x offset from origin to left edge of glyph box
        //   y_bearing — y offset from origin to top edge  (usually negative)
        //   width, height — bounding box size
        //
        // To centre the bounding box on (cx, cy):
        //   tx = cx - x_bearing - width/2
        //   ty = cy - y_bearing - height/2
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

    // Slightly darker pill behind the search text so it stays readable.
    cr.set_source_rgba(0.0, 0.0, 0.0, 0.55);
    cr.rectangle(0.0, bar_y, w as f64, bar_h);
    cr.fill().unwrap();

    // Draw a thin separator line above the bar.
    cr.set_source_rgba(1.0, 1.0, 1.0, 0.12);
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
