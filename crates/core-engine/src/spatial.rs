//! 2D spatial types for the paper-doll renderer and PALS/MOLLE grid math.
//!
//! All coordinates are percentages of the platform canvas so sprites scale with
//! the render target; `z_index` drives draw order.

use serde::{Deserialize, Serialize};
use thiserror::Error;

/// A sprite anchor rectangle in canvas percentage space.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AnchorBox2D {
    /// Horizontal origin, as a percentage of canvas width.
    pub x_pct: f32,
    /// Vertical origin, as a percentage of canvas height.
    pub y_pct: f32,
    /// Width, as a percentage of canvas width.
    pub width_pct: f32,
    /// Height, as a percentage of canvas height.
    pub height_pct: f32,
    /// Draw order: higher values render above lower ones.
    pub z_index: i16,
}

impl AnchorBox2D {
    /// Create a new anchor box.
    pub const fn new(
        x_pct: f32,
        y_pct: f32,
        width_pct: f32,
        height_pct: f32,
        z_index: i16,
    ) -> Self {
        Self {
            x_pct,
            y_pct,
            width_pct,
            height_pct,
            z_index,
        }
    }

    /// Compose a child anchor (expressed relative to this box) into canvas
    /// space.
    ///
    /// The child's percentages are interpreted against this box's area, and
    /// `z_index` values are summed with saturation so nested mounts stack.
    pub fn compose(&self, child: &AnchorBox2D) -> AnchorBox2D {
        AnchorBox2D {
            x_pct: self.x_pct + child.x_pct * self.width_pct / 100.0,
            y_pct: self.y_pct + child.y_pct * self.height_pct / 100.0,
            width_pct: child.width_pct * self.width_pct / 100.0,
            height_pct: child.height_pct * self.height_pct / 100.0,
            z_index: self.z_index.saturating_add(child.z_index),
        }
    }
}

/// Failures produced by PALS/MOLLE occupancy math and placement.
#[derive(Debug, Clone, PartialEq, Eq, Error, Serialize, Deserialize)]
pub enum SpatialError {
    /// The footprint extends past the edge of the grid.
    #[error("PALS footprint {item_cols}x{item_rows} at ({col_idx}, {row_idx}) exceeds grid bounds {cols}x{rows}")]
    OutOfBounds {
        /// Grid width in cells.
        cols: u8,
        /// Grid height in cells.
        rows: u8,
        /// Requested column index.
        col_idx: u8,
        /// Requested row index.
        row_idx: u8,
        /// Requested footprint width in cells.
        item_cols: u8,
        /// Requested footprint height in cells.
        item_rows: u8,
    },
    /// The footprint overlaps cells that are already occupied.
    #[error("PALS footprint {item_cols}x{item_rows} at ({col_idx}, {row_idx}) overlaps occupied cells")]
    AlreadyOccupied {
        /// Requested column index.
        col_idx: u8,
        /// Requested row index.
        row_idx: u8,
        /// Requested footprint width in cells.
        item_cols: u8,
        /// Requested footprint height in cells.
        item_rows: u8,
    },
    /// The footprint has a zero dimension.
    #[error("PALS footprint must be at least 1x1, got {item_cols}x{item_rows}")]
    InvalidFootprint {
        /// Requested footprint width in cells.
        item_cols: u8,
        /// Requested footprint height in cells.
        item_rows: u8,
    },
    /// The target slot exposes no PALS grid.
    #[error("mount slot `{slot_id}` exposes no PALS grid")]
    NoPalsGrid {
        /// Identifier of the offending slot.
        slot_id: String,
    },
    /// The component declares no PALS footprint.
    #[error("component `{component}` declares no PALS footprint")]
    NoPalsFootprint {
        /// Name of the offending component.
        component: String,
    },
}

/// Occupancy state of a PALS/MOLLE webbing field.
///
/// `occupied` is row-major (`occupied[row][col]`, so `occupied.len() == rows`
/// and `occupied[row].len() == cols`). Prefer [`PalsMatrix::new`] to build a
/// consistent grid.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PalsMatrix {
    /// Number of columns (vertical PALS channels).
    pub cols: u8,
    /// Number of rows (horizontal webbing rows).
    pub rows: u8,
    /// Row-major cell occupancy flags.
    pub occupied: Vec<Vec<bool>>,
}

impl PalsMatrix {
    /// Create an empty `cols` x `rows` grid.
    pub fn new(cols: u8, rows: u8) -> Self {
        Self {
            cols,
            rows,
            occupied: vec![vec![false; cols as usize]; rows as usize],
        }
    }

    /// Read a single cell; out-of-grid coordinates count as occupied (i.e.
    /// unusable), and so do cells of a malformed `occupied` vector.
    pub fn is_occupied(&self, col_idx: u8, row_idx: u8) -> bool {
        if col_idx >= self.cols || row_idx >= self.rows {
            return true;
        }
        self.occupied
            .get(row_idx as usize)
            .map_or(true, |row| row.get(col_idx as usize).copied().unwrap_or(true))
    }

    /// Whether an `item_cols` x `item_rows` footprint fits unoccupied at
    /// `(col_idx, row_idx)`.
    pub fn can_fit(&self, col_idx: u8, row_idx: u8, item_cols: u8, item_rows: u8) -> bool {
        if item_cols == 0 || item_rows == 0 {
            return false;
        }
        if !self.bounds_ok(col_idx, row_idx, item_cols, item_rows) {
            return false;
        }
        (row_idx..row_idx + item_rows)
            .all(|r| (col_idx..col_idx + item_cols).all(|c| !self.is_occupied(c, r)))
    }

    /// Occupy the cells of an `item_cols` x `item_rows` footprint at
    /// `(col_idx, row_idx)`, or fail with a [`SpatialError`].
    pub fn mount(
        &mut self,
        col_idx: u8,
        row_idx: u8,
        item_cols: u8,
        item_rows: u8,
    ) -> Result<(), SpatialError> {
        if item_cols == 0 || item_rows == 0 {
            return Err(SpatialError::InvalidFootprint {
                item_cols,
                item_rows,
            });
        }
        if !self.bounds_ok(col_idx, row_idx, item_cols, item_rows) {
            return Err(SpatialError::OutOfBounds {
                cols: self.cols,
                rows: self.rows,
                col_idx,
                row_idx,
                item_cols,
                item_rows,
            });
        }
        if !self.can_fit(col_idx, row_idx, item_cols, item_rows) {
            return Err(SpatialError::AlreadyOccupied {
                col_idx,
                row_idx,
                item_cols,
                item_rows,
            });
        }
        for r in row_idx..row_idx + item_rows {
            for c in col_idx..col_idx + item_cols {
                if let Some(cell) = self
                    .occupied
                    .get_mut(r as usize)
                    .and_then(|row| row.get_mut(c as usize))
                {
                    *cell = true;
                }
            }
        }
        Ok(())
    }

    /// Whether the footprint's end edges stay inside the grid (checked
    /// arithmetic: `u8` sums must not wrap).
    fn bounds_ok(&self, col_idx: u8, row_idx: u8, item_cols: u8, item_rows: u8) -> bool {
        col_idx
            .checked_add(item_cols)
            .is_some_and(|end| end <= self.cols)
            && row_idx
                .checked_add(item_rows)
                .is_some_and(|end| end <= self.rows)
    }

    /// Release the cells of a previously-mounted footprint — the inverse of
    /// [`PalsMatrix::mount`].
    ///
    /// Clamped to the grid and infallible by design: releasing is cleanup of
    /// state that was validated at mount time, so it can never fail.
    pub fn release(&mut self, col_idx: u8, row_idx: u8, item_cols: u8, item_rows: u8) {
        let col_start = col_idx.min(self.cols);
        let row_start = row_idx.min(self.rows);
        let col_end = col_idx.saturating_add(item_cols).min(self.cols);
        let row_end = row_idx.saturating_add(item_rows).min(self.rows);
        for row in row_start..row_end {
            for col in col_start..col_end {
                if let Some(cell) = self
                    .occupied
                    .get_mut(row as usize)
                    .and_then(|cells| cells.get_mut(col as usize))
                {
                    *cell = false;
                }
            }
        }
    }
}
