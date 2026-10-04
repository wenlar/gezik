//! Where entries are on screen, in content coordinates (y grows down from the first row,
//! not counting scrolling): rows in the list view, cells in the grid. Used for rubber-band
//! selection and keyboard moves.

use std::ops::Range;

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Geometry {
    List { row_height: f32 },
    Grid { cell_width: f32, cell_height: f32, columns: usize },
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Rect {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
}

impl Rect {
    /// The rectangle between two corners, in any order.
    pub fn from_points(x0: f32, y0: f32, x1: f32, y1: f32) -> Rect {
        Rect { x: x0.min(x1), y: y0.min(y1), width: (x1 - x0).abs(), height: (y1 - y0).abs() }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Move {
    Up,
    Down,
    Left,
    Right,
    PageUp,
    PageDown,
    Home,
    End,
}

impl Geometry {
    /// A grid as wide as `width`: as many whole cells per row as fit, at least one.
    pub fn grid(width: f32, cell_width: f32, cell_height: f32) -> Geometry {
        let columns = if cell_width > 0.0 { (width / cell_width).floor().max(1.0) as usize } else { 1 };
        Geometry::Grid { cell_width, cell_height, columns }
    }

    /// Entries per row: 1 in the list.
    pub fn per_row(&self) -> usize {
        match self {
            Geometry::List { .. } => 1,
            Geometry::Grid { columns, .. } => (*columns).max(1),
        }
    }

    pub fn row_height(&self) -> f32 {
        match self {
            Geometry::List { row_height } => *row_height,
            Geometry::Grid { cell_height, .. } => *cell_height,
        }
    }

    pub fn row_count(&self, count: usize) -> usize {
        count.div_ceil(self.per_row())
    }

    pub fn row_of(&self, index: usize) -> usize {
        index / self.per_row()
    }

    /// The entries `rect` touches, as ascending ranges (one per grid row). In the list a
    /// row counts wherever the rectangle crosses it horizontally.
    pub fn items_in_rect(&self, rect: Rect, count: usize) -> Vec<Range<usize>> {
        let row_height = self.row_height();
        let rows = self.row_count(count);
        let bottom = rect.y + rect.height;
        if count == 0 || row_height <= 0.0 || bottom <= 0.0 {
            return Vec::new();
        }
        let first_row = (rect.y.max(0.0) / row_height).floor() as usize;
        let last_row = ((bottom / row_height).ceil() as usize).saturating_sub(1).min(rows - 1);
        if first_row >= rows || first_row > last_row {
            return Vec::new();
        }
        match *self {
            #[allow(clippy::single_range_in_vec_init)]
            Geometry::List { .. } => vec![first_row..(last_row + 1).min(count)],
            Geometry::Grid { cell_width, columns, .. } => {
                let columns = columns.max(1);
                let right = rect.x + rect.width;
                if right <= 0.0 || rect.x >= columns as f32 * cell_width || cell_width <= 0.0 {
                    return Vec::new();
                }
                let c0 = (rect.x.max(0.0) / cell_width).floor() as usize;
                let c1 = ((right / cell_width).ceil() as usize).saturating_sub(1).min(columns - 1);
                (first_row..=last_row)
                    .filter_map(|r| {
                        let (start, end) = (r * columns + c0, (r * columns + c1 + 1).min(count));
                        (start < end).then_some(start..end)
                    })
                    .collect()
            }
        }
    }

    /// Where the focus goes from `from` for a key. With no focus yet every key goes to the
    /// first entry (End to the last). Left and Right do nothing in the list (`None`).
    pub fn step(&self, from: Option<usize>, mv: Move, count: usize, page_rows: usize) -> Option<usize> {
        if count == 0 {
            return None;
        }
        let last = count - 1;
        let grid = matches!(self, Geometry::Grid { .. });
        let Some(i) = from.filter(|i| *i < count) else {
            return match mv {
                Move::Left | Move::Right if !grid => None,
                Move::End => Some(last),
                _ => Some(0),
            };
        };
        let n = self.per_row();
        let page = page_rows.max(1) * n;
        Some(match mv {
            Move::Up => i.checked_sub(n).unwrap_or(if grid { i } else { 0 }),
            Move::Down if i + n <= last => i + n,
            // From the row above a shorter last row, Down lands on the last entry.
            Move::Down if self.row_of(i) < self.row_of(last) => last,
            Move::Down => i,
            Move::Left if grid => i.saturating_sub(1),
            Move::Right if grid => (i + 1).min(last),
            Move::Left | Move::Right => return None,
            Move::PageUp => i.saturating_sub(page),
            Move::PageDown => (i + page).min(last),
            Move::Home => 0,
            Move::End => last,
        })
    }
}

#[cfg(test)]
#[allow(clippy::single_range_in_vec_init)]
mod tests {
    use super::*;

    const LIST: Geometry = Geometry::List { row_height: 26.0 };

    fn grid(columns: usize) -> Geometry {
        Geometry::Grid { cell_width: 100.0, cell_height: 120.0, columns }
    }

    #[test]
    fn grid_fits_whole_cells_and_at_least_one() {
        assert_eq!(Geometry::grid(450.0, 100.0, 120.0).per_row(), 4);
        assert_eq!(Geometry::grid(50.0, 100.0, 120.0).per_row(), 1);
        assert_eq!(grid(4).row_count(9), 3);
        assert_eq!(grid(4).row_of(8), 2);
    }

    #[test]
    fn list_rect_hits_rows_it_crosses() {
        assert_eq!(LIST.items_in_rect(Rect::from_points(10.0, 30.0, 5.0, 80.0), 100), [1..4]);
        assert_eq!(LIST.items_in_rect(Rect { x: 0.0, y: 0.0, width: 1.0, height: 26.0 }, 100), [0..1]);
        assert_eq!(LIST.items_in_rect(Rect { x: 0.0, y: 2000.0, width: 1.0, height: 10.0 }, 10), []);
        assert_eq!(LIST.items_in_rect(Rect { x: 0.0, y: -50.0, width: 1.0, height: 40.0 }, 10), []);
        assert_eq!(LIST.items_in_rect(Rect { x: 0.0, y: -50.0, width: 1.0, height: 60.0 }, 10), [0..1]);
        assert_eq!(LIST.items_in_rect(Rect { x: 0.0, y: 0.0, width: 1.0, height: 9999.0 }, 3), [0..3]);
    }

    #[test]
    fn grid_rect_hits_cells_per_row() {
        // Columns 1–2 of rows 0–1 in a 4-column grid.
        let rect = Rect::from_points(150.0, 10.0, 250.0, 130.0);
        assert_eq!(grid(4).items_in_rect(rect, 100), [1..3, 5..7]);
        // The last row is short: cells past the end are not hit.
        assert_eq!(grid(4).items_in_rect(Rect::from_points(0.0, 250.0, 399.0, 260.0), 10), [8..10]);
        // Right of the last column: nothing.
        assert_eq!(grid(4).items_in_rect(Rect::from_points(420.0, 0.0, 500.0, 50.0), 100), []);
    }

    #[test]
    fn list_keys_move_by_rows() {
        assert_eq!(LIST.step(Some(5), Move::Up, 10, 3), Some(4));
        assert_eq!(LIST.step(Some(0), Move::Up, 10, 3), Some(0));
        assert_eq!(LIST.step(Some(9), Move::Down, 10, 3), Some(9));
        assert_eq!(LIST.step(Some(5), Move::PageDown, 10, 3), Some(8));
        assert_eq!(LIST.step(Some(5), Move::PageUp, 10, 3), Some(2));
        assert_eq!(LIST.step(Some(5), Move::Left, 10, 3), None);
        assert_eq!(LIST.step(None, Move::Down, 10, 3), Some(0));
        assert_eq!(LIST.step(None, Move::End, 10, 3), Some(9));
        assert_eq!(LIST.step(None, Move::Down, 0, 3), None);
    }

    #[test]
    fn grid_keys_move_by_cells_and_rows() {
        let g = grid(4);
        assert_eq!(g.step(Some(5), Move::Left, 10, 2), Some(4));
        assert_eq!(g.step(Some(5), Move::Right, 10, 2), Some(6));
        assert_eq!(g.step(Some(9), Move::Right, 10, 2), Some(9));
        assert_eq!(g.step(Some(5), Move::Up, 10, 2), Some(1));
        assert_eq!(g.step(Some(1), Move::Up, 10, 2), Some(1), "top row stays");
        assert_eq!(g.step(Some(1), Move::Down, 10, 2), Some(5));
        assert_eq!(g.step(Some(7), Move::Down, 10, 2), Some(9), "onto the short last row");
        assert_eq!(g.step(Some(9), Move::Down, 10, 2), Some(9));
        assert_eq!(g.step(Some(0), Move::PageDown, 100, 2), Some(8));
        assert_eq!(g.step(Some(3), Move::Home, 10, 2), Some(0));
        assert_eq!(g.step(Some(3), Move::End, 10, 2), Some(9));
    }
}
