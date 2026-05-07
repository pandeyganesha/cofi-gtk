// ─────────────────────────────────────────────────────────────────────────────
// nav.rs
//
// Arrow-key navigation over the visible subset of the app grid.
//
// The grid is CONCEPTUAL — every app has a fixed (row, col) position derived
// from its index:
//   row = index / cols
//   col = index % cols
//
// Only "visible" apps (those matching the query) can be selected.  Arrow keys
// jump to the nearest visible app in the requested direction.
//
// DOWN  → go to the next row that has a visible app; pick the one whose column
//          is closest to the current column.  Wrap to the topmost row if at
//          the bottom.
// UP    → mirror of DOWN.
// RIGHT → same row, next column.  If none in this row, jump to the leftmost
//          item of the next row.  Wrap to the very first item.
// LEFT  → same row, previous column.  If none, jump to the rightmost item of
//          the previous row.  Wrap to the very last item.
// ─────────────────────────────────────────────────────────────────────────────

#[derive(Clone, Copy)]
pub enum Direction {
    Up,
    Down,
    Left,
    Right,
}

/// Move the selection from `current` in `direction` among the `items` slice.
///
/// `items` — indices (into the global app list) that are currently navigatable.
/// `current` — the currently selected app index (must be in `items`).
/// `cols` — number of grid columns (used to derive row/col positions).
///
/// Returns the new selected index.
pub fn navigate(items: &[usize], current: usize, cols: usize, dir: Direction) -> usize {
    if items.is_empty() {
        return current;
    }

    // Ensure cols is never zero (guard against edge cases).
    let cols = cols.max(1);

    let cur_row = current / cols;
    let cur_col = current % cols;

    match dir {
        // ── DOWN ─────────────────────────────────────────────────────────────
        Direction::Down => {
            // Find the smallest row that is strictly below cur_row AND has
            // at least one visible item.
            let next_row = items.iter()
                .map(|&i| i / cols)
                .filter(|&r| r > cur_row)
                .min();

            let target_row = match next_row {
                Some(r) => r,
                None => {
                    // We are on the last populated row → wrap to the topmost.
                    items.iter().map(|&i| i / cols).min().unwrap()
                }
            };

            nearest_in_row(items, target_row, cur_col, cols)
        }

        // ── UP ───────────────────────────────────────────────────────────────
        Direction::Up => {
            let prev_row = items.iter()
                .map(|&i| i / cols)
                .filter(|&r| r < cur_row)
                .max();

            let target_row = match prev_row {
                Some(r) => r,
                None => {
                    // We are on the topmost row → wrap to the bottommost.
                    items.iter().map(|&i| i / cols).max().unwrap()
                }
            };

            nearest_in_row(items, target_row, cur_col, cols)
        }

        // ── RIGHT ────────────────────────────────────────────────────────────
        Direction::Right => {
            // Same row, first col > cur_col.
            let same_row_right = items.iter().copied()
                .filter(|&i| i / cols == cur_row && i % cols > cur_col)
                .min_by_key(|&i| i % cols);

            if let Some(next) = same_row_right {
                return next;
            }

            // Nothing to the right in this row → go to next row's leftmost item.
            let next_row = items.iter().map(|&i| i / cols)
                .filter(|&r| r > cur_row)
                .min();

            let target_row = match next_row {
                Some(r) => r,
                None => items.iter().map(|&i| i / cols).min().unwrap(), // wrap
            };

            // Leftmost item in that row.
            items.iter().copied()
                .filter(|&i| i / cols == target_row)
                .min_by_key(|&i| i % cols)
                .unwrap_or(current)
        }

        // ── LEFT ─────────────────────────────────────────────────────────────
        Direction::Left => {
            // Same row, last col < cur_col.
            let same_row_left = items.iter().copied()
                .filter(|&i| i / cols == cur_row && i % cols < cur_col)
                .max_by_key(|&i| i % cols);

            if let Some(prev) = same_row_left {
                return prev;
            }

            // Nothing to the left in this row → go to prev row's rightmost item.
            let prev_row = items.iter().map(|&i| i / cols)
                .filter(|&r| r < cur_row)
                .max();

            let target_row = match prev_row {
                Some(r) => r,
                None => items.iter().map(|&i| i / cols).max().unwrap(), // wrap
            };

            // Rightmost item in that row.
            items.iter().copied()
                .filter(|&i| i / cols == target_row)
                .max_by_key(|&i| i % cols)
                .unwrap_or(current)
        }
    }
}

// ── Internal helper ───────────────────────────────────────────────────────────

/// Among all `items` in the given `row`, return the one whose column is
/// closest to `target_col`.  Ties are broken by smaller column index.
fn nearest_in_row(items: &[usize], row: usize, target_col: usize, cols: usize) -> usize {
    items.iter().copied()
        .filter(|&i| i / cols == row)
        .min_by_key(|&i| {
            let c = i % cols;
            // Distance from target column, ties broken by column index itself.
            (c.abs_diff(target_col), c)
        })
        .unwrap_or(0)
}
