//! The size map (spec 11 §10): a folder tree that keeps folders only, and its squarified
//! treemap (Bruls, Huizing, van Wijk) laid out two levels deep. Pure: no file system.

use std::time::SystemTime;

/// Most rectangles a map shows (spec §10.2): the software renderer's frame time.
pub const MAX_TILES: usize = 2_000;
/// An item whose share of its level is smaller than 4×4 px joins "N small items".
pub const MIN_AREA: f32 = 16.0;
/// A rectangle this big gets a label (name and size); a folder's nested items start below it.
pub const LABEL_W: f32 = 64.0;
pub const LABEL_H: f32 = 20.0;
/// No node.
pub const NONE: u32 = u32::MAX;

#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Rect {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
}

impl Rect {
    pub fn contains(&self, x: f32, y: f32) -> bool {
        x >= self.x && y >= self.y && x < self.x + self.w && y < self.y + self.h
    }

    fn center(&self) -> (f32, f32) {
        (self.x + self.w / 2.0, self.y + self.h / 2.0)
    }
}

/// `sizes` laid out in `area`, each rectangle's area in proportion to its size, in the order
/// of `sizes` (they need not be sorted). A size of 0 gets an empty rectangle.
pub fn squarify(sizes: &[u64], area: Rect) -> Vec<Rect> {
    let mut out = vec![Rect { x: area.x, y: area.y, w: 0.0, h: 0.0 }; sizes.len()];
    let total: f64 = sizes.iter().map(|&s| s as f64).sum();
    if total <= 0.0 || area.w <= 0.0 || area.h <= 0.0 {
        return out;
    }
    let mut order: Vec<usize> = (0..sizes.len()).filter(|&i| sizes[i] > 0).collect();
    order.sort_by(|&a, &b| sizes[b].cmp(&sizes[a]).then(a.cmp(&b)));
    let scale = f64::from(area.w) * f64::from(area.h) / total;
    let area_of = |i: usize| sizes[order[i]] as f64 * scale;
    let (mut x, mut y, mut w, mut h) = (f64::from(area.x), f64::from(area.y), f64::from(area.w), f64::from(area.h));
    // The worst aspect ratio of a row of areas `max`..`min` adding up to `sum` along `side`.
    let worst = |max: f64, min: f64, sum: f64, side: f64| {
        let (s2, l2) = (sum * sum, side * side);
        (l2 * max / s2).max(s2 / (l2 * min))
    };
    let mut start = 0;
    while start < order.len() {
        let side = w.min(h);
        let (max, mut sum, mut end) = (area_of(start), area_of(start), start + 1);
        let mut ratio = worst(max, max, sum, side);
        // The row grows while that makes its worst rectangle squarer (sorted: the newest is the smallest).
        while end < order.len() {
            let next = worst(max, area_of(end), sum + area_of(end), side);
            if next > ratio {
                break;
            }
            (ratio, sum, end) = (next, sum + area_of(end), end + 1);
        }
        // The last row takes what is left, so rounding leaves no gap.
        let thick = if end == order.len() { if w >= h { w } else { h } } else { sum / side };
        let mut along = 0.0;
        for i in start..end {
            let len = if end == order.len() && i + 1 == end { side - along } else { area_of(i) / thick };
            out[order[i]] = if w >= h {
                Rect { x: x as f32, y: (y + along) as f32, w: thick as f32, h: len as f32 }
            } else {
                Rect { x: (x + along) as f32, y: y as f32, w: len as f32, h: thick as f32 }
            };
            along += len;
        }
        if w >= h {
            (x, w) = (x + thick, w - thick);
        } else {
            (y, h) = (y + thick, h - thick);
        }
        start = end;
    }
    out
}

/// A folder of the tree.
#[derive(Debug, Clone)]
pub struct Node {
    /// Its name in the tree's arena (start, length).
    name: (u32, u32),
    pub parent: u32,
    first_child: u32,
    next: u32,
    /// Its modified time as its parent's listing gave it (the folder sizes cache's stamp).
    pub stamp: Option<SystemTime>,
    /// Its own files.
    pub own_bytes: u64,
    pub own_files: u64,
    /// It could not be read, or was not gone into (a cloud folder, too deep).
    pub unread: bool,
    /// Under it, as [`SizeTree::sum`] last added them up.
    pub bytes: u64,
    pub files: u64,
    pub folders: u64,
    pub partial: bool,
}

/// A folder tree (spec §10.1): folders only, with what their files add up to; the root's own
/// files one by one. A node always comes after its parent.
#[derive(Debug, Clone)]
pub struct SizeTree {
    names: String,
    nodes: Vec<Node>,
    /// The root's files (name, size).
    pub files: Vec<(String, u64)>,
}

impl SizeTree {
    pub fn new(root_name: &str) -> SizeTree {
        let mut tree = SizeTree { names: String::new(), nodes: Vec::new(), files: Vec::new() };
        tree.push(NONE, root_name, None);
        tree
    }

    fn push(&mut self, parent: u32, name: &str, stamp: Option<SystemTime>) -> u32 {
        let id = self.nodes.len() as u32;
        let at = (self.names.len() as u32, name.len() as u32);
        self.names.push_str(name);
        self.nodes.push(Node {
            name: at,
            parent,
            first_child: NONE,
            next: NONE,
            stamp,
            own_bytes: 0,
            own_files: 0,
            unread: false,
            bytes: 0,
            files: 0,
            folders: 0,
            partial: false,
        });
        id
    }

    /// A folder `name` in `parent`; its id.
    pub fn add(&mut self, parent: u32, name: &str, stamp: Option<SystemTime>) -> u32 {
        let id = self.push(parent, name, stamp);
        self.nodes[id as usize].next = std::mem::replace(&mut self.nodes[parent as usize].first_child, id);
        id
    }

    pub fn len(&self) -> usize {
        self.nodes.len()
    }

    pub fn is_empty(&self) -> bool {
        self.nodes.is_empty()
    }

    pub fn node(&self, id: u32) -> &Node {
        &self.nodes[id as usize]
    }

    pub fn node_mut(&mut self, id: u32) -> &mut Node {
        &mut self.nodes[id as usize]
    }

    pub fn name(&self, id: u32) -> &str {
        let (start, len) = self.nodes[id as usize].name;
        &self.names[start as usize..(start + len) as usize]
    }

    pub fn children(&self, id: u32) -> impl Iterator<Item = u32> + '_ {
        let some = |c: u32| (c != NONE).then_some(c);
        std::iter::successors(some(self.nodes[id as usize].first_child), move |&c| some(self.nodes[c as usize].next))
    }

    /// The names from the root's child down to `id` (the root itself: none).
    pub fn path(&self, id: u32) -> Vec<&str> {
        let mut names = Vec::new();
        let mut at = id;
        while at != 0 && at != NONE {
            names.push(self.name(at));
            at = self.nodes[at as usize].parent;
        }
        names.reverse();
        names
    }

    /// Takes `id` out of its parent (it went to the trash); the next [`SizeTree::sum`] leaves it out.
    pub fn remove(&mut self, id: u32) {
        let parent = self.nodes[id as usize].parent;
        if parent == NONE {
            return;
        }
        let next = self.nodes[id as usize].next;
        if self.nodes[parent as usize].first_child == id {
            self.nodes[parent as usize].first_child = next;
        } else {
            let before = self.children(parent).find(|&c| self.nodes[c as usize].next == id);
            if let Some(before) = before {
                self.nodes[before as usize].next = next;
            }
        }
        self.nodes[id as usize].parent = NONE;
    }

    /// Takes the root's file `index` out (it went to the trash).
    pub fn remove_file(&mut self, index: usize) {
        if index < self.files.len() {
            let (_, size) = self.files.remove(index);
            let root = &mut self.nodes[0];
            root.own_bytes = root.own_bytes.saturating_sub(size);
            root.own_files = root.own_files.saturating_sub(1);
        }
    }

    /// Adds every folder's totals up again from the folders' own files (a scan going on gives
    /// partial ones). A removed node's totals do not reach the folders above it.
    pub fn sum(&mut self) {
        for node in &mut self.nodes {
            (node.bytes, node.files, node.folders, node.partial) = (node.own_bytes, node.own_files, 0, node.unread);
        }
        for id in (1..self.nodes.len()).rev() {
            let node = &self.nodes[id];
            let (parent, bytes, files, folders, partial) =
                (node.parent, node.bytes, node.files, node.folders, node.partial);
            if parent == NONE {
                continue;
            }
            let up = &mut self.nodes[parent as usize];
            up.bytes = up.bytes.saturating_add(bytes);
            up.files += files;
            up.folders += folders + 1;
            up.partial |= partial;
        }
    }
}

/// What a rectangle of the map is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum What {
    Folder(u32),
    /// The root's file (an index into `SizeTree::files`).
    File(usize),
    /// A folder's own files, as one block.
    Files(u32),
    /// This many items too small to show.
    Small(usize),
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Tile {
    pub rect: Rect,
    pub what: What,
    /// 0: an item of the folder shown; 1: an item of one of its folders.
    pub depth: u8,
    pub bytes: u64,
}

impl Tile {
    /// Whether it is big enough for a label.
    pub fn labelled(&self) -> bool {
        self.rect.w >= LABEL_W && self.rect.h >= LABEL_H
    }
}

/// The items of folder `id` (its folders, and its files: one by one for the root, else as one
/// block), the biggest first, then by name: the same tree always gives the same map.
fn level_items(tree: &SizeTree, id: u32) -> Vec<(What, u64)> {
    let mut items: Vec<(What, u64, &str)> =
        tree.children(id).map(|c| (What::Folder(c), tree.node(c).bytes, tree.name(c))).collect();
    if id == 0 {
        items.extend(tree.files.iter().enumerate().map(|(i, (name, size))| (What::File(i), *size, name.as_str())));
    } else if tree.node(id).own_bytes > 0 {
        items.push((What::Files(id), tree.node(id).own_bytes, ""));
    }
    items.retain(|(_, size, _)| *size > 0);
    items.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.2.cmp(b.2)));
    items.into_iter().map(|(what, size, _)| (what, size)).collect()
}

/// `items` (sorted, biggest first) in `area`, at most `budget` rectangles: those whose share
/// is under `MIN_AREA`, and those past the budget, become one "N small items".
fn place(mut items: Vec<(What, u64)>, area: Rect, budget: usize) -> Vec<(Rect, What, u64)> {
    let total: f64 = items.iter().map(|(_, s)| *s as f64).sum();
    let per_byte = f64::from(area.w) * f64::from(area.h) / total.max(1.0);
    let mut keep = items.iter().take_while(|(_, s)| *s as f64 * per_byte >= f64::from(MIN_AREA)).count();
    if items.len() > budget {
        keep = keep.min(budget.saturating_sub(1));
    }
    // One small item stays itself.
    if items.len() - keep > 1 {
        let small = items.split_off(keep);
        items.push((What::Small(small.len()), small.iter().map(|(_, s)| *s).sum()));
    }
    let sizes: Vec<u64> = items.iter().map(|(_, s)| *s).collect();
    squarify(&sizes, area).into_iter().zip(items).map(|(rect, (what, size))| (rect, what, size)).collect()
}

/// The map of folder `focus` in `area` (spec §10.2): its items, and each of its folders' items
/// inside that folder's rectangle (under its label), at most `MAX_TILES` in all. A nested
/// item comes after its folder.
pub fn layout(tree: &SizeTree, focus: u32, area: Rect) -> Vec<Tile> {
    let tile = |(rect, what, bytes): (Rect, What, u64), depth| Tile { rect, what, depth, bytes };
    let mut tiles: Vec<Tile> =
        place(level_items(tree, focus), area, MAX_TILES).into_iter().map(|t| tile(t, 0)).collect();
    for i in 0..tiles.len() {
        let Tile { rect, what: What::Folder(id), .. } = tiles[i] else { continue };
        let top = if rect.w >= LABEL_W && rect.h >= LABEL_H * 2.0 { LABEL_H } else { 2.0 };
        let inner = Rect { x: rect.x + 2.0, y: rect.y + top, w: rect.w - 4.0, h: rect.h - top - 2.0 };
        let budget = MAX_TILES - tiles.len();
        let items = level_items(tree, id);
        // Only its own files: one block over the folder would say nothing more.
        if inner.w < 8.0 || inner.h < 8.0 || budget < 2 || items.len() < 2 {
            continue;
        }
        tiles.extend(place(items, inner, budget).into_iter().map(|t| tile(t, 1)));
    }
    tiles
}

/// The rectangle at `x`, `y`: the deepest one there.
pub fn hit(tiles: &[Tile], x: f32, y: f32) -> Option<usize> {
    tiles.iter().rposition(|t| t.rect.contains(x, y))
}

/// The rectangle of the same depth nearest to `from` in the direction `dx`, `dy` (one of
/// them 0, the other ±1): the arrow keys.
pub fn neighbor(tiles: &[Tile], from: usize, dx: f32, dy: f32) -> Option<usize> {
    let here = tiles.get(from)?;
    let (fx, fy) = here.rect.center();
    tiles
        .iter()
        .enumerate()
        .filter(|(i, t)| *i != from && t.depth == here.depth)
        .filter_map(|(i, t)| {
            let (cx, cy) = t.rect.center();
            let (ahead, aside) = ((cx - fx) * dx + (cy - fy) * dy, ((cx - fx) * dy).abs() + ((cy - fy) * dx).abs());
            (ahead > 0.5).then_some((i, ahead + 2.0 * aside))
        })
        .min_by(|a, b| a.1.total_cmp(&b.1))
        .map(|(i, _)| i)
}

#[cfg(test)]
mod tests {
    use super::*;

    const AREA: Rect = Rect { x: 0.0, y: 0.0, w: 800.0, h: 500.0 };

    fn close(a: f32, b: f32) -> bool {
        (a - b).abs() <= 0.01 * a.abs().max(b.abs()).max(1.0)
    }

    #[test]
    fn areas_follow_the_sizes_and_fill_the_area() {
        let sizes = [600, 600, 400, 400, 300, 200, 200, 100, 6];
        let rects = squarify(&sizes, AREA);
        let total: u64 = sizes.iter().sum();
        let mut covered = 0.0;
        for (rect, size) in rects.iter().zip(sizes) {
            let want = size as f32 / total as f32 * AREA.w * AREA.h;
            assert!(close(rect.w * rect.h, want), "{rect:?} for {size}");
            assert!(rect.x >= -0.01 && rect.y >= -0.01 && rect.x + rect.w <= 800.01 && rect.y + rect.h <= 500.01);
            covered += rect.w * rect.h;
        }
        assert!(close(covered, AREA.w * AREA.h));
        // No two overlap.
        for (i, a) in rects.iter().enumerate() {
            for b in &rects[i + 1..] {
                let x = (a.x + a.w).min(b.x + b.w) - a.x.max(b.x);
                let y = (a.y + a.h).min(b.y + b.h) - a.y.max(b.y);
                assert!(x <= 0.01 || y <= 0.01, "{a:?} {b:?}");
            }
        }
    }

    #[test]
    fn rectangles_stay_near_square() {
        // The paper's example: 6 6 4 3 2 2 1 in 6×4 — every ratio ≤ 3.
        let rects = squarify(&[6, 6, 4, 3, 2, 2, 1], Rect { x: 0.0, y: 0.0, w: 6.0, h: 4.0 });
        for r in &rects {
            assert!(r.w.max(r.h) / r.w.min(r.h) <= 3.0, "{r:?}");
        }
        let many: Vec<u64> = (1..=200).collect();
        for r in squarify(&many, AREA) {
            assert!(r.w.max(r.h) / r.w.min(r.h) <= 6.0, "{r:?}");
        }
    }

    #[test]
    fn zero_sizes_and_one_item() {
        let rects = squarify(&[0, 5, 0], AREA);
        assert_eq!((rects[0].w * rects[0].h, rects[2].w * rects[2].h), (0.0, 0.0));
        assert_eq!(rects[1], AREA, "one item takes it all");
        assert!(squarify(&[0, 0], AREA).iter().all(|r| r.w == 0.0));
        assert!(squarify(&[], AREA).is_empty());
    }

    /// A root with `folders` (name, own bytes, sub-folders' own bytes) and `files`.
    fn tree(folders: &[(&str, u64, &[u64])], files: &[u64]) -> SizeTree {
        let mut tree = SizeTree::new("root");
        for (name, own, subs) in folders {
            let id = tree.add(0, name, None);
            tree.node_mut(id).own_bytes = *own;
            for (i, sub) in subs.iter().enumerate() {
                let sub_id = tree.add(id, &format!("{name}{i}"), None);
                tree.node_mut(sub_id).own_bytes = *sub;
            }
        }
        tree.files = files.iter().enumerate().map(|(i, s)| (format!("f{i}"), *s)).collect();
        tree.node_mut(0).own_bytes = files.iter().sum();
        tree.sum();
        tree
    }

    #[test]
    fn totals_add_up_and_a_removed_folder_drops_out() {
        let mut t = tree(&[("a", 10, &[5, 5]), ("b", 0, &[])], &[3]);
        assert_eq!((t.node(0).bytes, t.node(0).folders, t.node(0).files), (23, 4, 0));
        let a = t.children(0).find(|&c| t.name(c) == "a").unwrap();
        assert_eq!(t.path(t.children(a).next().unwrap()), ["a", "a1"]);
        t.remove(a);
        t.sum();
        assert_eq!(t.node(0).bytes, 3);
        t.remove_file(0);
        t.sum();
        assert_eq!(t.node(0).bytes, 0);
    }

    #[test]
    fn the_map_nests_one_level_with_the_files_block() {
        let t = tree(&[("a", 100, &[300, 600]), ("b", 50, &[])], &[200]);
        let tiles = layout(&t, 0, AREA);
        let top: Vec<What> = tiles.iter().filter(|t| t.depth == 0).map(|t| t.what).collect();
        assert_eq!(top.len(), 3, "a, the file, b");
        assert!(matches!(top[0], What::Folder(_)) && top[1] == What::File(0));
        let nested: Vec<What> = tiles.iter().filter(|t| t.depth == 1).map(|t| t.what).collect();
        assert_eq!(nested.len(), 3, "a's two folders and its files: {nested:?}");
        assert!(nested.iter().any(|w| matches!(w, What::Files(_))));
        // A nested item lies inside its folder, under the label.
        let a = tiles[0].rect;
        for t in tiles.iter().filter(|t| t.depth == 1) {
            assert!(t.rect.x >= a.x && t.rect.y >= a.y + LABEL_H - 0.01 && t.rect.x + t.rect.w <= a.x + a.w + 0.01);
        }
        assert_eq!(hit(&tiles, a.x + a.w / 2.0, a.y + 5.0), Some(0), "the label strip is the folder's");
        let inner = tiles.iter().position(|t| t.depth == 1).unwrap();
        let r = tiles[inner].rect;
        assert_eq!(hit(&tiles, r.x + r.w / 2.0, r.y + r.h / 2.0), Some(inner), "the deepest");
    }

    #[test]
    fn tiny_items_join_one_block() {
        let mut files = vec![1_000_000u64];
        files.extend(std::iter::repeat_n(1, 50));
        let tiles = layout(&tree(&[], &files), 0, AREA);
        assert_eq!(tiles.len(), 2);
        assert_eq!(tiles[1].what, What::Small(50));
        assert_eq!(tiles[1].bytes, 50);
        // One tiny item is shown as itself.
        let tiles = layout(&tree(&[], &[1_000_000, 1]), 0, AREA);
        assert_eq!(tiles[1].what, What::File(1));
    }

    #[test]
    fn at_most_two_thousand_rectangles() {
        let big = Rect { x: 0.0, y: 0.0, w: 4000.0, h: 4000.0 };
        let files = vec![10u64; 5000];
        let tiles = layout(&tree(&[], &files), 0, big);
        assert_eq!(tiles.len(), MAX_TILES);
        assert_eq!(tiles.last().unwrap().what, What::Small(5000 - MAX_TILES + 1));
        // Nested levels share what is left.
        let subs = vec![1u64; 400];
        let folders: Vec<(String, u64, &[u64])> = (0..10).map(|i| (format!("d{i}"), 0, subs.as_slice())).collect();
        let folders: Vec<(&str, u64, &[u64])> = folders.iter().map(|(n, o, s)| (n.as_str(), *o, *s)).collect();
        let tiles = layout(&tree(&folders, &[]), 0, big);
        assert!(tiles.len() <= MAX_TILES, "{}", tiles.len());
    }

    #[test]
    fn the_same_tree_gives_the_same_map_whatever_the_scan_order() {
        let a = tree(&[("x", 5, &[]), ("y", 5, &[]), ("z", 9, &[1, 1])], &[5]);
        let b = tree(&[("z", 9, &[1, 1]), ("y", 5, &[]), ("x", 5, &[])], &[5]);
        let names = |t: &SizeTree| -> Vec<(String, Rect)> {
            layout(t, 0, AREA)
                .iter()
                .map(|tile| {
                    let name = match tile.what {
                        What::Folder(id) => t.path(id).join("/"),
                        What::Files(id) => format!("{}/files", t.path(id).join("/")),
                        other => format!("{other:?}"),
                    };
                    (name, tile.rect)
                })
                .collect()
        };
        assert_eq!(names(&a), names(&b));
        assert_eq!(layout(&a, 0, AREA), layout(&a, 0, AREA));
    }

    #[test]
    fn labels_need_room() {
        let tile = |w, h| Tile { rect: Rect { x: 0.0, y: 0.0, w, h }, what: What::Small(1), depth: 0, bytes: 1 };
        assert!(tile(64.0, 20.0).labelled());
        assert!(!tile(63.0, 40.0).labelled() && !tile(100.0, 19.0).labelled());
    }

    #[test]
    fn arrows_go_to_the_nearest_rectangle_that_way() {
        let tile = |x, y| Tile { rect: Rect { x, y, w: 10.0, h: 10.0 }, what: What::Small(1), depth: 0, bytes: 1 };
        let tiles = [tile(0.0, 0.0), tile(10.0, 0.0), tile(20.0, 0.0), tile(0.0, 10.0), tile(10.0, 10.0)];
        assert_eq!(neighbor(&tiles, 0, 1.0, 0.0), Some(1));
        assert_eq!(neighbor(&tiles, 0, 0.0, 1.0), Some(3));
        assert_eq!(neighbor(&tiles, 4, -1.0, 0.0), Some(3));
        assert_eq!(neighbor(&tiles, 4, 0.0, -1.0), Some(1));
        assert_eq!(neighbor(&tiles, 2, 1.0, 0.0), None, "the edge");
    }

    #[test]
    fn two_thousand_items_lay_out_quickly() {
        let files: Vec<u64> = (0..5000u64).map(|i| (i * 7919) % 100_000 + 1).collect();
        let t = tree(&[], &files);
        let started = std::time::Instant::now();
        let tiles = layout(&t, 0, Rect { x: 0.0, y: 0.0, w: 1280.0, h: 800.0 });
        assert!(tiles.len() <= MAX_TILES);
        // Spec §10.2: ≤ 30 ms; the debug build gets room.
        assert!(started.elapsed() < std::time::Duration::from_millis(300), "{:?}", started.elapsed());
    }
}
