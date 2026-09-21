//! Spatial area-of-interest (AoI) grid — the 1k-player architecture's
//! interest-management foundation (milestone 1).
//!
//! Each act's world (the square ±`WORLD_BOUNDS`) is divided into square cells.
//! Every live entity is filed in exactly one cell; the grid is maintained
//! incrementally, so per-tick maintenance costs O(moved entities) — a linear
//! scan with an O(1) early-out per entity — and never O(players × entities).
//! Per-client visibility queries walk only the cells intersecting the
//! client's AoI circle instead of scanning every entity in the zone.

use antediluvia_protocol::{EntityId, WORLD_BOUNDS};
use glam::Vec2;

/// Cell edge length in world units. Chosen at roughly a third of the default
/// AoI radius (1400): a max-radius query touches ~5.5×5.5 cells — small enough
/// to cull well, large enough that a 20 Hz tick almost never moves an entity
/// more than one cell (180 u/s × 0.05 s = 9 u).
pub const CELL_SIZE: f32 = 512.0;

/// Cells per side: 7200 / 512 = 14.06 → 15 cells cover ±3600 (positions are
/// clamped to ±WORLD_BOUNDS anyway).
pub const GRID_DIM: usize = 15;
/// Total cells per zone.
pub const GRID_CELLS: usize = GRID_DIM * GRID_DIM; // 225

/// Cell index for a world position, in row-major order (`cy * GRID_DIM + cx`).
/// Clamped, so out-of-range positions still file deterministically.
pub fn cell_of(x: f32, y: f32) -> usize {
    let cx = (((x + WORLD_BOUNDS) / CELL_SIZE) as i32).clamp(0, GRID_DIM as i32 - 1) as usize;
    let cy = (((y + WORLD_BOUNDS) / CELL_SIZE) as i32).clamp(0, GRID_DIM as i32 - 1) as usize;
    cy * GRID_DIM + cx
}

/// (column, row) of a cell index.
pub fn cell_xy(cell: usize) -> (usize, usize) {
    (cell % GRID_DIM, cell / GRID_DIM)
}

/// Indices of every cell whose rectangle intersects the circle
/// (`center`, `radius`), in row-major order. A client "sees" its own cell
/// plus these neighbors — this is what replaces the per-client scan of all
/// zone entities.
pub fn cells_in_radius(center: Vec2, radius: f32) -> impl Iterator<Item = usize> {
    let r = radius.max(0.0);
    let x0 =
        (((center.x - r + WORLD_BOUNDS) / CELL_SIZE).floor() as i32).clamp(0, GRID_DIM as i32 - 1);
    let x1 =
        (((center.x + r + WORLD_BOUNDS) / CELL_SIZE).floor() as i32).clamp(0, GRID_DIM as i32 - 1);
    let y0 =
        (((center.y - r + WORLD_BOUNDS) / CELL_SIZE).floor() as i32).clamp(0, GRID_DIM as i32 - 1);
    let y1 =
        (((center.y + r + WORLD_BOUNDS) / CELL_SIZE).floor() as i32).clamp(0, GRID_DIM as i32 - 1);
    (y0..=y1).flat_map(move |cy| {
        (x0..=x1)
            .filter(move |cx| {
                // Keep the cell iff the closest point of its rectangle to the
                // center lies within the radius.
                let rx0 = *cx as f32 * CELL_SIZE - WORLD_BOUNDS;
                let ry0 = cy as f32 * CELL_SIZE - WORLD_BOUNDS;
                let dx = (rx0 - center.x).max(0.0).max(center.x - (rx0 + CELL_SIZE));
                let dy = (ry0 - center.y).max(0.0).max(center.y - (ry0 + CELL_SIZE));
                dx * dx + dy * dy <= r * r
            })
            .map(move |cx| cy as usize * GRID_DIM + cx as usize)
    })
}

/// Per-zone spatial index: every live entity id filed in exactly one cell.
/// The sim owns all mutation (insert/remove on spawn/despawn, refiles when an
/// entity crosses a cell boundary); readers only query.
#[derive(Debug)]
pub struct AoiGrid {
    cells: Vec<Vec<EntityId>>,
}

impl Default for AoiGrid {
    /// A default grid is a usable empty grid, not a zero-cell stub.
    fn default() -> Self {
        Self::new()
    }
}

impl AoiGrid {
    pub fn new() -> Self {
        Self {
            cells: vec![Vec::new(); GRID_CELLS],
        }
    }

    /// Entity ids currently filed in `cell`.
    pub fn cell_entities(&self, cell: usize) -> &[EntityId] {
        &self.cells[cell]
    }

    pub fn insert(&mut self, id: EntityId, cell: usize) {
        debug_assert!(cell < GRID_CELLS);
        self.cells[cell].push(id);
    }

    pub fn remove(&mut self, id: EntityId, cell: usize) {
        debug_assert!(cell < GRID_CELLS);
        let v = &mut self.cells[cell];
        if let Some(i) = v.iter().position(|e| *e == id) {
            v.swap_remove(i);
        }
    }

    pub fn move_entity(&mut self, id: EntityId, from: usize, to: usize) {
        if from == to {
            return;
        }
        self.remove(id, from);
        self.insert(id, to);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cell_of_maps_corners_and_center() {
        // World spans ±3600; cell (0,0) covers [-3600, -3088).
        assert_eq!(cell_of(-3600.0, -3600.0), 0);
        assert_eq!(cell_xy(0), (0, 0));
        // Center of the world.
        let c = cell_of(0.0, 0.0);
        assert_eq!(cell_xy(c), (7, 7));
        // Far corner clamps into the last cell; wild coordinates clamp to the edge.
        assert_eq!(cell_of(3600.0, 3600.0), GRID_CELLS - 1);
        assert_eq!(cell_of(99999.0, -99999.0), GRID_DIM - 1);
        // A step of one cell moves the index by exactly one.
        assert_eq!(cell_of(-3600.0 + CELL_SIZE, -3600.0), 1);
        assert_eq!(cell_of(-3600.0, -3600.0 + CELL_SIZE), GRID_DIM);
    }

    #[test]
    fn cells_in_radius_covers_own_cell_and_neighbors() {
        // Center of cell (7,7); tiny radius → just the cell containing the point.
        let center = Vec2::ZERO;
        let own = cell_of(0.0, 0.0);
        let cells: Vec<usize> = cells_in_radius(center, 1.0).collect();
        assert_eq!(cells, vec![own]);

        // Radius 600 from a cell's center reaches all 8 neighbors (512-wide
        // cells; the nearest two-away cell edge is 768 units off) but nothing
        // further — exactly the 3×3 block.
        let center = Vec2::new(240.0, 240.0); // center of cell (7,7)
        assert_eq!(cell_xy(cell_of(240.0, 240.0)), (7, 7));
        let cells: Vec<usize> = cells_in_radius(center, 600.0).collect();
        assert_eq!(cells.len(), 9, "expected 3x3, got {}", cells.len());
        let (cx, cy) = cell_xy(own);
        for dy in -1..=1 {
            for dx in -1..=1 {
                let idx = (cy as i32 + dy) as usize * GRID_DIM + (cx as i32 + dx) as usize;
                assert!(cells.contains(&idx), "missing neighbor cell {idx}");
            }
        }
    }

    #[test]
    fn grid_insert_remove_move() {
        let mut g = AoiGrid::new();
        g.insert(1, 7);
        g.insert(2, 7);
        g.insert(3, 8);
        assert_eq!(g.cell_entities(7), &[1, 2]);
        g.move_entity(2, 7, 8);
        assert_eq!(g.cell_entities(7), &[1]);
        assert!(g.cell_entities(8).contains(&2));
        g.remove(1, 7);
        assert!(g.cell_entities(7).is_empty());
        // Removing an id that isn't there is a no-op, not a panic.
        g.remove(99, 7);
    }
}
