//! Where entries are on screen, in content coordinates (y grows down from the first row,
//! not counting scrolling): rows in the list view, cells in the grid. Used for rubber-band
//! selection and keyboard moves.

use std::ops::Range;

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Geometry {
    List {
        row_height: f32,
    },
    /// `left`: where the first column starts, in the list's coordinates (the grid is inset).
    Grid {
        cell_width: f32,
        cell_height: f32,
        columns: usize,
        left: f32,
    },
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

    /// Whether point (`x`, `y`) is inside (the left and top edges are, the others are not).
    pub fn contains(&self, x: f32, y: f32) -> bool {
        x >= self.x && x < self.x + self.width && y >= self.y && y < self.y + self.height
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
        Geometry::Grid { cell_width, cell_height, columns, left: 0.0 }
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
            Geometry::Grid { cell_width, columns, left, .. } => {
                let columns = columns.max(1);
                // From the first column's edge (the grid is inset from the list's).
                let (x, right) = (rect.x - left, rect.x + rect.width - left);
                if right <= 0.0 || x >= columns as f32 * cell_width || cell_width <= 0.0 {
                    return Vec::new();
                }
                let c0 = (x.max(0.0) / cell_width).floor() as usize;
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

/// One group of a grouped view (spec 10 §6): entries `start..start + len`, together in the
/// listing; a closed one shows only its header.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Span {
    pub start: usize,
    pub len: usize,
    pub collapsed: bool,
}

impl Span {
    fn end(&self) -> usize {
        self.start + self.len
    }
}

/// A line of a grouped view: group `g`'s header, or entries.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Line {
    Header(usize),
    Cells(Range<usize>),
}

/// The lines of a grouped view: per group a header line, then its entries `per_row` a line
/// (none while closed). Every line is as high as the others (spec 10 §6.2: in the grid a
/// header takes a cell line). shortcut: each call walks the groups; keep the first line of each
/// if a folder with thousands of types scrolls slowly.
#[derive(Debug, Clone, Copy)]
pub struct Lines<'a> {
    pub spans: &'a [Span],
    pub per_row: usize,
}

impl Lines<'_> {
    fn per(&self) -> usize {
        self.per_row.max(1)
    }

    /// Lines of entries in group `g` (none while closed).
    fn rows_in(&self, g: usize) -> usize {
        let s = &self.spans[g];
        if s.collapsed { 0 } else { s.len.div_ceil(self.per()) }
    }

    pub fn count(&self) -> usize {
        (0..self.spans.len()).map(|g| 1 + self.rows_in(g)).sum()
    }

    pub fn line(&self, n: usize) -> Option<Line> {
        let mut first = 0;
        for g in 0..self.spans.len() {
            let lines = 1 + self.rows_in(g);
            if n < first + lines {
                return Some(if n == first { Line::Header(g) } else { Line::Cells(self.row(g, n - first - 1)) });
            }
            first += lines;
        }
        None
    }

    /// The line entry `index` is on and its column; `None` in a closed group or past the end.
    pub fn line_of(&self, index: usize) -> Option<(usize, usize)> {
        let mut first = 0;
        for (g, s) in self.spans.iter().enumerate() {
            if (s.start..s.end()).contains(&index) {
                let k = index - s.start;
                return (!s.collapsed).then_some((first + 1 + k / self.per(), k % self.per()));
            }
            first += 1 + self.rows_in(g);
        }
        None
    }

    /// The entries on lines `lines`: from the first to past the last (a closed group between
    /// counts in; headers have none).
    pub fn entries_on(&self, lines: Range<usize>) -> Range<usize> {
        let mut out: Option<Range<usize>> = None;
        for n in lines {
            match self.line(n) {
                Some(Line::Cells(r)) => out = Some(out.map_or(r.clone(), |o| o.start..r.end)),
                Some(Line::Header(_)) => {}
                None => break,
            }
        }
        out.unwrap_or(0..0)
    }

    /// The entries `rect` touches (content coordinates), as `Geometry::items_in_rect` does
    /// without groups; header lines have none.
    pub fn items_in_rect(&self, geometry: &Geometry, rect: Rect) -> Vec<Range<usize>> {
        let h = geometry.row_height();
        let (bottom, count) = (rect.y + rect.height, self.count());
        if count == 0 || h <= 0.0 || bottom <= 0.0 {
            return Vec::new();
        }
        let first = (rect.y.max(0.0) / h).floor() as usize;
        let last = ((bottom / h).ceil() as usize).saturating_sub(1).min(count - 1);
        if first > last {
            return Vec::new();
        }
        let columns = match *geometry {
            Geometry::List { .. } => None,
            Geometry::Grid { cell_width, left, .. } => {
                let (x, right) = (rect.x - left, rect.x + rect.width - left);
                if right <= 0.0 || cell_width <= 0.0 || x >= self.per() as f32 * cell_width {
                    return Vec::new();
                }
                let c0 = (x.max(0.0) / cell_width).floor() as usize;
                let c1 = ((right / cell_width).ceil() as usize).saturating_sub(1).min(self.per() - 1);
                Some((c0, c1))
            }
        };
        (first..=last)
            .filter_map(|n| match self.line(n)? {
                Line::Header(_) => None,
                Line::Cells(r) => match columns {
                    None => Some(r),
                    Some((c0, c1)) => {
                        let (start, end) = (r.start + c0, (r.start + c1 + 1).min(r.end));
                        (start < end).then_some(start..end)
                    }
                },
            })
            .collect()
    }

    /// Entries `r`'th line in group `g`.
    fn row(&self, g: usize, r: usize) -> Range<usize> {
        let s = &self.spans[g];
        let a = s.start + r * self.per();
        a..(a + self.per()).min(s.end())
    }

    fn next_row(&self, (g, r): (usize, usize)) -> Option<(usize, usize)> {
        if r + 1 < self.rows_in(g) {
            return Some((g, r + 1));
        }
        (g + 1..self.spans.len()).find(|&h| self.rows_in(h) > 0).map(|h| (h, 0))
    }

    fn prev_row(&self, (g, r): (usize, usize)) -> Option<(usize, usize)> {
        if r > 0 {
            return Some((g, r - 1));
        }
        (0..g).rev().find(|&h| self.rows_in(h) > 0).map(|h| (h, self.rows_in(h) - 1))
    }

    /// Where the focus goes from `from` for a key, as `Geometry::step` does without groups:
    /// lines of entries only (headers and closed groups are skipped). A focus in a closed
    /// group counts as none. `None` if no entry shows, or for Left/Right in the list.
    pub fn step(&self, from: Option<usize>, mv: Move, page_rows: usize, grid: bool) -> Option<usize> {
        let first_g = (0..self.spans.len()).find(|&g| self.rows_in(g) > 0)?;
        let last_g = (0..self.spans.len()).rev().find(|&g| self.rows_in(g) > 0)?;
        let (first, last) = (self.spans[first_g].start, self.row(last_g, self.rows_in(last_g) - 1).end - 1);
        let located = from.and_then(|i| {
            let g = self.spans.iter().position(|s| !s.collapsed && (s.start..s.end()).contains(&i))?;
            Some((i, (g, (i - self.spans[g].start) / self.per())))
        });
        let Some((i, at)) = located else {
            return match mv {
                Move::Left | Move::Right if !grid => None,
                Move::End => Some(last),
                _ => Some(first),
            };
        };
        let here = self.row(at.0, at.1);
        let col = i - here.start;
        let land = |(g, r): (usize, usize)| {
            let row = self.row(g, r);
            (row.start + col).min(row.end - 1)
        };
        let walk = |n: usize, down: bool| {
            let mut p = at;
            for _ in 0..n {
                match if down { self.next_row(p) } else { self.prev_row(p) } {
                    Some(q) => p = q,
                    None => break,
                }
            }
            p
        };
        let page = page_rows.max(1);
        Some(match mv {
            Move::Up => land(walk(1, false)),
            Move::Down => land(walk(1, true)),
            Move::PageUp => land(walk(page, false)),
            Move::PageDown => land(walk(page, true)),
            Move::Left if grid => {
                if i > here.start {
                    i - 1
                } else {
                    self.prev_row(at).map_or(i, |p| self.row(p.0, p.1).end - 1)
                }
            }
            Move::Right if grid => {
                if i + 1 < here.end {
                    i + 1
                } else {
                    self.next_row(at).map_or(i, |p| self.row(p.0, p.1).start)
                }
            }
            Move::Left | Move::Right => return None,
            Move::Home => first,
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
        Geometry::Grid { cell_width: 100.0, cell_height: 120.0, columns, left: 0.0 }
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
    fn an_inset_grid_starts_at_its_left_edge() {
        let g = Geometry::Grid { cell_width: 100.0, cell_height: 120.0, columns: 4, left: 6.0 };
        // In the gap left of the first column: nothing.
        assert_eq!(g.items_in_rect(Rect::from_points(0.0, 10.0, 5.0, 20.0), 100), []);
        // Just inside the first column, and across into the second (it starts at 106).
        assert_eq!(g.items_in_rect(Rect::from_points(6.0, 10.0, 7.0, 20.0), 100), [0..1]);
        assert_eq!(g.items_in_rect(Rect::from_points(100.0, 10.0, 107.0, 20.0), 100), [0..2]);
        // Right of the last column (it ends at 406): nothing.
        assert_eq!(g.items_in_rect(Rect::from_points(407.0, 0.0, 450.0, 50.0), 100), []);
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

    fn spans(list: &[(usize, usize, bool)]) -> Vec<Span> {
        list.iter().map(|&(start, len, collapsed)| Span { start, len, collapsed }).collect()
    }

    #[test]
    fn list_lines_have_a_header_per_group() {
        // 0..3 open, 3..5 closed, 5..9 open.
        let s = spans(&[(0, 3, false), (3, 2, true), (5, 4, false)]);
        let lines = Lines { spans: &s, per_row: 1 };
        assert_eq!(lines.count(), 10);
        assert_eq!(lines.line(0), Some(Line::Header(0)));
        assert_eq!(lines.line(1), Some(Line::Cells(0..1)));
        assert_eq!(lines.line(4), Some(Line::Header(1)), "a closed group shows only its header");
        assert_eq!(lines.line(5), Some(Line::Header(2)));
        assert_eq!(lines.line(9), Some(Line::Cells(8..9)));
        assert_eq!(lines.line(10), None);
        assert_eq!(lines.line_of(0), Some((1, 0)));
        assert_eq!(lines.line_of(3), None, "closed");
        assert_eq!(lines.line_of(5), Some((6, 0)));
        assert_eq!(lines.line_of(9), None, "past the end");
        assert_eq!(lines.entries_on(0..5), 0..3);
        assert_eq!(lines.entries_on(4..6), 0..0, "headers only");
        assert_eq!(lines.entries_on(8..20), 7..9);
    }

    #[test]
    fn grid_lines_start_each_group_on_a_new_line() {
        let s = spans(&[(0, 3, false), (3, 4, false)]);
        let lines = Lines { spans: &s, per_row: 2 };
        assert_eq!(lines.count(), 6);
        assert_eq!(lines.line(2), Some(Line::Cells(2..3)), "the group's short last line");
        assert_eq!(lines.line(3), Some(Line::Header(1)));
        assert_eq!(lines.line(4), Some(Line::Cells(3..5)));
        assert_eq!(lines.line_of(4), Some((4, 1)));
        assert_eq!(lines.line_of(6), Some((5, 1)));
    }

    #[test]
    fn list_keys_skip_headers_and_closed_groups() {
        let s = spans(&[(0, 3, false), (3, 2, true), (5, 4, false)]);
        let lines = Lines { spans: &s, per_row: 1 };
        let step = |from, mv| lines.step(from, mv, 3, false);
        assert_eq!(step(Some(2), Move::Down), Some(5));
        assert_eq!(step(Some(5), Move::Up), Some(2));
        assert_eq!(step(Some(0), Move::Up), Some(0));
        assert_eq!(step(Some(8), Move::Down), Some(8));
        assert_eq!(step(Some(0), Move::PageDown), Some(5), "three lines of entries");
        assert_eq!(step(Some(8), Move::PageUp), Some(5));
        assert_eq!(step(Some(6), Move::Home), Some(0));
        assert_eq!(step(Some(0), Move::End), Some(8));
        assert_eq!(step(None, Move::Down), Some(0));
        assert_eq!(step(None, Move::End), Some(8));
        assert_eq!(step(Some(3), Move::Down), Some(0), "a focus in a closed group counts as none");
        assert_eq!(step(Some(2), Move::Left), None);
        let closed = spans(&[(0, 3, true)]);
        assert_eq!(Lines { spans: &closed, per_row: 1 }.step(Some(1), Move::Down, 3, false), None, "nothing shows");
    }

    #[test]
    fn grid_keys_cross_groups() {
        let s = spans(&[(0, 3, false), (3, 4, false)]);
        let lines = Lines { spans: &s, per_row: 2 };
        let step = |from, mv| lines.step(Some(from), mv, 1, true);
        assert_eq!(step(1, Move::Down), Some(2), "onto the short last line");
        assert_eq!(step(2, Move::Down), Some(3), "into the next group");
        assert_eq!(step(4, Move::Up), Some(2), "column kept, cut to the short line");
        assert_eq!(step(2, Move::Right), Some(3));
        assert_eq!(step(3, Move::Left), Some(2));
        assert_eq!(step(6, Move::Right), Some(6), "the last entry stays");
        assert_eq!(step(0, Move::Left), Some(0));
        assert_eq!(step(5, Move::Down), Some(5), "the last line stays");
    }

    #[test]
    fn a_rubber_band_skips_header_lines() {
        let s = spans(&[(0, 3, false), (3, 4, false)]);
        let lines = Lines { spans: &s, per_row: 2 };
        let g = grid(2);
        // Column 1 of lines 1-2 (y 120-359): entry 1; line 2 has no column 1.
        assert_eq!(lines.items_in_rect(&g, Rect::from_points(150.0, 130.0, 160.0, 250.0)), [1..2]);
        // Over the second header only.
        assert_eq!(lines.items_in_rect(&g, Rect::from_points(0.0, 370.0, 199.0, 380.0)), []);
        // Lines 1-4, both columns.
        assert_eq!(lines.items_in_rect(&g, Rect::from_points(0.0, 125.0, 199.0, 590.0)), [0..2, 2..3, 3..5]);
        let list = spans(&[(0, 2, false), (2, 2, true), (4, 2, false)]);
        let lines = Lines { spans: &list, per_row: 1 };
        // Lines 0-5: header, 0, 1, closed header, header, 4.
        assert_eq!(lines.items_in_rect(&LIST, Rect::from_points(0.0, 0.0, 10.0, 26.0 * 6.0)), [0..1, 1..2, 4..5]);
    }
}
