//! 2D spatial types for the paper-doll renderer, 3D-ready section frames, and
//! PALS/MOLLE dual-footprint grid math.
//!
//! All coordinates are percentages of the platform canvas so sprites scale with
//! the render target; `z_index` drives draw order.

use serde::{Deserialize, Serialize};
use thiserror::Error;

/// A 3D plane coordinate frame for converting a 2D `KitSection` surface onto
/// a 3D rig mesh.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct SectionFrame3D {
    /// Outward surface normal `[x, y, z]`.
    pub normal: [f32; 3],
    /// Local 3D origin `[x, y, z]` in normalized rig space.
    pub origin_3d: [f32; 3],
    /// Up vector `[x, y, z]` of the 2D section plane.
    pub up_3d: [f32; 3],
}

/// PALS attachment footprint of a component, in 1" grid cells.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct PalsFootprint {
    /// Attachment footprint width in PALS columns.
    pub cols: u8,
    /// Attachment footprint height in PALS rows.
    pub rows: u8,
}

/// Physical material coverage footprint of a component in 1" PALS grid cells,
/// relative to the top-left `AnchorCell` of its `PalsFootprint`.
///
/// A pouch or tool may weave into `attach.cols x attach.rows` MOLLE channels
/// while its body/fabric spans `cols x rows` cells starting at
/// `(anchor_col + offset_col, anchor_row + offset_row)`. In-bounds covered
/// cells block overlapping gear, while out-of-bounds coverage cells are
/// allowed to overhang past the outer edge of the PALS grid.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct MaterialCoverage {
    /// Total covered width in PALS columns.
    pub cols: u8,
    /// Total covered height in PALS rows.
    pub rows: u8,
    /// Column offset from the attachment anchor cell (usually `<= 0`).
    #[serde(default)]
    pub offset_col: i8,
    /// Row offset from the attachment anchor cell (usually `<= 0`).
    #[serde(default)]
    pub offset_row: i8,
}

impl MaterialCoverage {
    /// Construct a material coverage footprint with zero offset.
    pub const fn new(cols: u8, rows: u8) -> Self {
        Self {
            cols,
            rows,
            offset_col: 0,
            offset_row: 0,
        }
    }

    /// Construct a material coverage footprint with explicit `(col, row)` offset.
    pub const fn with_offset(cols: u8, rows: u8, offset_col: i8, offset_row: i8) -> Self {
        Self {
            cols,
            rows,
            offset_col,
            offset_row,
        }
    }
}

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
/// and `occupied[row].len() == cols`), true whenever a cell is either woven
/// by an attachment strap or covered by in-bounds material overhang.
/// `covered` tracks cells covered exclusively by material overhang beyond the
/// attachment straps.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PalsMatrix {
    /// Number of columns (vertical PALS channels).
    pub cols: u8,
    /// Number of rows (horizontal webbing rows).
    pub rows: u8,
    /// Row-major cell occupancy flags (attachment OR material coverage).
    pub occupied: Vec<Vec<bool>>,
    /// Row-major material overhang coverage flags (optional in legacy JSON).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub covered: Vec<Vec<bool>>,
}

impl PalsMatrix {
    /// Create an empty `cols` x `rows` grid.
    pub fn new(cols: u8, rows: u8) -> Self {
        Self {
            cols,
            rows,
            occupied: vec![vec![false; cols as usize]; rows as usize],
            covered: Vec::new(),
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

    /// Whether `(col_idx, row_idx)` is marked as covered by material overhang.
    pub fn is_covered(&self, col_idx: u8, row_idx: u8) -> bool {
        if col_idx >= self.cols || row_idx >= self.rows {
            return false;
        }
        self.covered
            .get(row_idx as usize)
            .and_then(|row| row.get(col_idx as usize).copied())
            .unwrap_or(false)
    }

    /// Whether an `item_cols` x `item_rows` footprint fits unoccupied at
    /// `(col_idx, row_idx)`.
    pub fn can_fit(&self, col_idx: u8, row_idx: u8, item_cols: u8, item_rows: u8) -> bool {
        self.can_fit_with_coverage(
            col_idx,
            row_idx,
            PalsFootprint {
                cols: item_cols,
                rows: item_rows,
            },
            None,
        )
    }

    /// Whether an `attach` footprint and optional `coverage` footprint fit
    /// at `(col_idx, row_idx)`.
    ///
    /// Rules:
    /// 1. `attach` must have non-zero dimensions and lie strictly inside the
    ///    grid bounds `0..cols` and `0..rows`.
    /// 2. Every cell in `attach` must be unoccupied (`!is_occupied`).
    /// 3. If `coverage` is present, its in-bounds cells must also be
    ///    unoccupied (`!is_occupied`), while out-of-bounds coverage cells are
    ///    permitted as physical overhang past the grid edge.
    pub fn can_fit_with_coverage(
        &self,
        col_idx: u8,
        row_idx: u8,
        attach: PalsFootprint,
        coverage: Option<MaterialCoverage>,
    ) -> bool {
        if attach.cols == 0 || attach.rows == 0 {
            return false;
        }
        if !self.bounds_ok(col_idx, row_idx, attach.cols, attach.rows) {
            return false;
        }
        let attach_free = (row_idx..row_idx + attach.rows)
            .all(|r| (col_idx..col_idx + attach.cols).all(|c| !self.is_occupied(c, r)));
        if !attach_free {
            return false;
        }
        if let Some(cov) = coverage {
            if cov.cols == 0 || cov.rows == 0 {
                return false;
            }
            for (c, r) in self.in_bounds_coverage_cells(col_idx, row_idx, cov) {
                if self.is_occupied(c, r) {
                    return false;
                }
            }
        }
        true
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
        self.mount_with_coverage(
            col_idx,
            row_idx,
            PalsFootprint {
                cols: item_cols,
                rows: item_rows,
            },
            None,
        )
    }

    /// Occupy the cells of an `attach` footprint plus optional `coverage`
    /// overhang at `(col_idx, row_idx)`.
    pub fn mount_with_coverage(
        &mut self,
        col_idx: u8,
        row_idx: u8,
        attach: PalsFootprint,
        coverage: Option<MaterialCoverage>,
    ) -> Result<(), SpatialError> {
        if attach.cols == 0 || attach.rows == 0 {
            return Err(SpatialError::InvalidFootprint {
                item_cols: attach.cols,
                item_rows: attach.rows,
            });
        }
        if let Some(cov) = coverage {
            if cov.cols == 0 || cov.rows == 0 {
                return Err(SpatialError::InvalidFootprint {
                    item_cols: cov.cols,
                    item_rows: cov.rows,
                });
            }
        }
        if !self.bounds_ok(col_idx, row_idx, attach.cols, attach.rows) {
            return Err(SpatialError::OutOfBounds {
                cols: self.cols,
                rows: self.rows,
                col_idx,
                row_idx,
                item_cols: attach.cols,
                item_rows: attach.rows,
            });
        }
        if !self.can_fit_with_coverage(col_idx, row_idx, attach, coverage) {
            return Err(SpatialError::AlreadyOccupied {
                col_idx,
                row_idx,
                item_cols: attach.cols,
                item_rows: attach.rows,
            });
        }

        for r in row_idx..row_idx + attach.rows {
            for c in col_idx..col_idx + attach.cols {
                if let Some(cell) = self
                    .occupied
                    .get_mut(r as usize)
                    .and_then(|row| row.get_mut(c as usize))
                {
                    *cell = true;
                }
            }
        }

        if let Some(cov) = coverage {
            let extra_cells: Vec<(u8, u8)> = self
                .in_bounds_coverage_cells(col_idx, row_idx, cov)
                .into_iter()
                .filter(|&(c, r)| {
                    !(c >= col_idx
                        && c < col_idx + attach.cols
                        && r >= row_idx
                        && r < row_idx + attach.rows)
                })
                .collect();
            if !extra_cells.is_empty() {
                self.ensure_covered_matrix();
                for (c, r) in extra_cells {
                    if let Some(cell) = self
                        .occupied
                        .get_mut(r as usize)
                        .and_then(|row| row.get_mut(c as usize))
                    {
                        *cell = true;
                    }
                    if let Some(cov_cell) = self
                        .covered
                        .get_mut(r as usize)
                        .and_then(|row| row.get_mut(c as usize))
                    {
                        *cov_cell = true;
                    }
                }
            }
        }
        Ok(())
    }

    /// Release the cells of a previously-mounted footprint — the inverse of
    /// [`PalsMatrix::mount`].
    pub fn release(&mut self, col_idx: u8, row_idx: u8, item_cols: u8, item_rows: u8) {
        self.release_with_coverage(
            col_idx,
            row_idx,
            PalsFootprint {
                cols: item_cols,
                rows: item_rows,
            },
            None,
        );
    }

    /// Release both attachment cells and in-bounds material coverage cells.
    pub fn release_with_coverage(
        &mut self,
        col_idx: u8,
        row_idx: u8,
        attach: PalsFootprint,
        coverage: Option<MaterialCoverage>,
    ) {
        let col_start = col_idx.min(self.cols);
        let row_start = row_idx.min(self.rows);
        let col_end = col_idx.saturating_add(attach.cols).min(self.cols);
        let row_end = row_idx.saturating_add(attach.rows).min(self.rows);
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
        if let Some(cov) = coverage {
            for (c, r) in self.in_bounds_coverage_cells(col_idx, row_idx, cov) {
                if let Some(cell) = self
                    .occupied
                    .get_mut(r as usize)
                    .and_then(|cells| cells.get_mut(c as usize))
                {
                    *cell = false;
                }
                if let Some(cov_cell) = self
                    .covered
                    .get_mut(r as usize)
                    .and_then(|cells| cells.get_mut(c as usize))
                {
                    *cov_cell = false;
                }
            }
            if self.covered.iter().all(|row| row.iter().all(|&b| !b)) {
                self.covered.clear();
            }
        }
    }

    /// Allocate `covered` lazily so grids without overhang serialize identically
    /// to legacy seeds.
    fn ensure_covered_matrix(&mut self) {
        if self.covered.len() != self.rows as usize {
            self.covered = vec![vec![false; self.cols as usize]; self.rows as usize];
        }
    }

    /// Enumerate all in-bounds `(col, row)` cells covered by `cov` when
    /// anchored at `(col_idx, row_idx)`.
    fn in_bounds_coverage_cells(
        &self,
        col_idx: u8,
        row_idx: u8,
        cov: MaterialCoverage,
    ) -> Vec<(u8, u8)> {
        let start_c = i16::from(col_idx) + i16::from(cov.offset_col);
        let start_r = i16::from(row_idx) + i16::from(cov.offset_row);
        let end_c = start_c + i16::from(cov.cols);
        let end_r = start_r + i16::from(cov.rows);
        let mut out = Vec::new();
        for r in start_r..end_r {
            for c in start_c..end_c {
                if c >= 0 && c < i16::from(self.cols) && r >= 0 && r < i16::from(self.rows) {
                    out.push((c as u8, r as u8));
                }
            }
        }
        out
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
}
