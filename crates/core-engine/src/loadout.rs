//! Stateful loadout assembly.
//!
//! A [`LoadoutDraft`] owns a live copy of a platform (PALS occupancy mutates
//! as items are equipped), the flat list of mounted items, and a monotonically
//! increasing mount-id counter. Every operation is deterministic: mount ids,
//! layer ordering, anchors, and the bill of materials depend only on the
//! sequence of equip/unequip calls.
//!
//! Slot addressing:
//! - Platform slots are addressed by their slot id (`"front_placard"`).
//! - Child slots exposed by a mounted component are addressed as
//!   `<mount_id>.<child_slot_id>` (e.g. `"m2.placard_front_pals"`); mount ids
//!   are returned by [`LoadoutDraft::equip_component`] and included in every
//!   [`Layer2D`] entry.
//!
//! Render layers are flat and deterministic:
//! [`PLATFORM_LAYER`] (0) < [`ADAPTER_LAYER`] (10) < [`PRIMARY_LAYER`] (20) <
//! [`CHILD_LAYER`] (30), so the paper-doll renderer draws the platform, then
//! adapter hardware, then the components hanging on it, then components
//! mounted onto other components.
//!
//! Adapter lifecycle: whenever a mount resolves through an adapter, the
//! adapter instance is auto-registered (tied to the mount it serves) and
//! garbage-collected again when that mount — or an ancestor of it — is
//! unequipped.

use crate::catalog::Catalog;
use crate::graph::{
    mount_on_pals, resolve_slot_mount, CompatibilityResult, IncompatibilityReason, MountSlot,
    PalsFootprint, PlatformSpec,
};
use crate::pricing::{best_offer, effective_price_cents, savings_cents, VendorOffer};
use crate::spatial::{AnchorBox2D, SpatialError};
use serde::{Deserialize, Serialize};
use thiserror::Error;

/// Render layer of the base platform.
pub const PLATFORM_LAYER: i16 = 0;
/// Render layer of auto-injected adapter hardware.
pub const ADAPTER_LAYER: i16 = 10;
/// Render layer of components mounted onto a platform slot.
pub const PRIMARY_LAYER: i16 = 20;
/// Render layer of components mounted onto another component's child slot.
pub const CHILD_LAYER: i16 = 30;

/// Every failure mode of loadout assembly.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Error)]
pub enum LoadoutError {
    /// The catalog holds no component with the requested SKU.
    #[error("unknown component sku `{sku}`")]
    UnknownComponent {
        /// The requested component SKU.
        sku: String,
    },
    /// No slot with the requested id or path exists on the platform or any
    /// mounted component.
    #[error("unknown mount slot `{slot_id}`")]
    UnknownSlot {
        /// The requested slot id or `<mount_id>.<child_slot_id>` path.
        slot_id: String,
    },
    /// No mounted item with the requested id.
    #[error("unknown mount id `{mount_id}`")]
    UnknownMount {
        /// The requested mount id.
        mount_id: String,
    },
    /// The component cannot mount onto the slot.
    #[error("component `{component_sku}` cannot mount on slot `{slot_id}`: {reason:?}")]
    IncompatibleComponent {
        /// The slot that was targeted.
        slot_id: String,
        /// The component that was rejected.
        component_sku: String,
        /// Why the mount is impossible.
        reason: IncompatibilityReason,
    },
    /// The PALS placement was rejected: out of bounds or cell collision.
    #[error("PALS placement failed on slot `{slot_id}`: {source}")]
    PalsCollision {
        /// The slot that was targeted.
        slot_id: String,
        /// The underlying spatial failure.
        #[source]
        source: SpatialError,
    },
    /// A footprinted component mounted on a PALS slot must declare its cell
    /// position.
    #[error("slot `{slot_id}` requires a PALS position for component `{component_sku}`")]
    MissingPalsPosition {
        /// The slot that was targeted.
        slot_id: String,
        /// The component that declared a footprint but no position.
        component_sku: String,
    },
    /// Adapters are auto-managed; only the components they serve can be
    /// unequipped directly.
    #[error("adapter `{sku}` ({mount_id}) is auto-managed; unequip the component it serves instead")]
    AutoManagedAdapter {
        /// The adapter's mount id.
        mount_id: String,
        /// The adapter's SKU.
        sku: String,
    },
}

/// One equipped record in a [`LoadoutDraft`]: either a user-placed component
/// or an auto-injected adapter bridge instance.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MountedItem {
    /// Unique mount id (`"m1"`, `"m2"`, ...), stable for the draft's lifetime.
    pub mount_id: String,
    /// SKU of the mounted component or adapter.
    pub sku: String,
    /// Product name (display only).
    pub name: String,
    /// Whether this item is an auto-injected adapter bridge.
    pub is_adapter: bool,
    /// Slot path this item occupies: a platform slot id (`"front_placard"`)
    /// or a child slot path (`"m2.placard_front_pals"`).
    pub slot_id: String,
    /// PALS cell position, when the item was placed on a grid.
    pub pals_position: Option<(u8, u8)>,
    /// PALS footprint, when the component declares one.
    pub pals_footprint: Option<PalsFootprint>,
    /// Adapter SKUs auto-registered for this mount (components only).
    pub required_adapters: Vec<String>,
    /// For adapters: the mount id of the component this instance serves.
    pub serves_mount: Option<String>,
    /// Canvas anchor for the 2D renderer.
    pub anchor: AnchorBox2D,
    /// Render layer: 0 platform, 10 adapter, 20 primary, 30 child.
    pub z_index: i16,
    /// MSRP of this item, snapshotted from the catalog at equip time.
    pub msrp_cents: u32,
    /// Vendor listings for this item, snapshotted at equip time so the BOM
    /// rollup needs no further catalog access.
    pub vendor_offers: Vec<VendorOffer>,
    /// Child slots this item exposes, with live PALS occupancy state.
    pub child_slots: Vec<MountSlot>,
    /// Mount ids of the items placed onto this item's child slots.
    pub child_mounts: Vec<String>,
}

/// One bill-of-materials line, aggregated per SKU. Pricing fields are
/// per-unit; multiply by `quantity` for the line total.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BomLineItem {
    /// SKU slug.
    pub sku: String,
    /// Product name (display only).
    pub name: String,
    /// How many instances of this SKU the loadout requires.
    pub quantity: u32,
    /// Whether this line is an adapter bridge or a component.
    pub is_adapter: bool,
    /// Per-unit manufacturer's suggested retail price, US cents.
    pub msrp_cents: u32,
    /// Per-unit best in-stock vendor offer, when one exists.
    pub best_offer: Option<VendorOffer>,
    /// Per-unit savings: MSRP minus the best in-stock price, clamped at zero
    /// (zero when no vendor holds stock).
    pub savings_cents: u32,
}

/// What a render layer represents.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LayerKind {
    /// The base platform paper doll.
    Platform,
    /// An auto-injected adapter bridge.
    Adapter,
    /// A user-placed component.
    Component,
}

/// One entry of the ordered 2D render stack.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Layer2D {
    /// Mount id (`"platform"` for the base layer).
    pub mount_id: String,
    /// SKU slug.
    pub sku: String,
    /// Product name (display only).
    pub name: String,
    /// What this layer represents.
    pub kind: LayerKind,
    /// Draw order (higher renders on top).
    pub z_index: i16,
    /// Canvas anchor.
    pub anchor: AnchorBox2D,
    /// Slot path the item occupies (empty for the platform layer).
    pub slot_id: String,
}

/// The complete evaluation of a loadout draft: render stack, bill of
/// materials, and diagnostics. This is the canonical API response shape.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LoadoutSummary {
    /// SKU of the base platform.
    pub platform_sku: String,
    /// Name of the base platform.
    pub platform_name: String,
    /// Whether every placement succeeded.
    pub valid: bool,
    /// Ordered 2D render stack: platform first, then by layer, then mount
    /// order.
    pub layers_2d: Vec<Layer2D>,
    /// Bill of materials, in first-mount order.
    pub bom: Vec<BomLineItem>,
    /// Total MSRP across the platform and every BOM line (qty-weighted),
    /// US cents.
    pub total_msrp_cents: u32,
    /// Total optimized build cost: platform plus every line at its best
    /// in-stock vendor price (MSRP where no stock exists), US cents.
    pub total_best_price_cents: u32,
    /// Total savings versus MSRP, clamped at zero, US cents.
    pub total_savings_cents: u32,
    /// Non-fatal advisories.
    pub warnings: Vec<String>,
    /// Fatal placement errors (filled in by API-level evaluation).
    pub errors: Vec<String>,
}

/// A stateful, deterministic loadout under construction.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LoadoutDraft {
    platform: PlatformSpec,
    mounted: Vec<MountedItem>,
    warnings: Vec<String>,
    mount_seq: u32,
}

impl LoadoutDraft {
    /// Start a new draft on `platform`. The platform's PALS grids start empty.
    pub fn new(platform: PlatformSpec) -> Self {
        Self {
            platform,
            mounted: Vec::new(),
            warnings: Vec::new(),
            mount_seq: 0,
        }
    }

    /// The base platform, with live PALS occupancy state.
    pub fn platform(&self) -> &PlatformSpec {
        &self.platform
    }

    /// All mounted items (components and adapters), in mount order.
    pub fn mounted(&self) -> &[MountedItem] {
        &self.mounted
    }

    /// Look up a mounted item by mount id.
    pub fn mounted_item(&self, mount_id: &str) -> Option<&MountedItem> {
        self.mounted.iter().find(|item| item.mount_id == mount_id)
    }

    /// Warnings accumulated so far.
    pub fn warnings(&self) -> &[String] {
        &self.warnings
    }

    /// Equip a component onto a platform slot or a mounted component's child
    /// slot.
    ///
    /// * `slot_id` — a platform slot id (`"front_placard"`) or a child slot
    ///   path (`"m2.placard_front_pals"`).
    /// * `pals_pos` — the `(col_idx, row_idx)` cell, required when the target
    ///   slot exposes a PALS grid *and* the component declares a footprint;
    ///   ignored otherwise (non-grid mounts position through their render
    ///   anchor).
    ///
    /// Resolution order:
    /// 1. The slot must exist on the base platform or on an already-mounted
    ///    component's `child_slots` ([`LoadoutError::UnknownSlot`]).
    /// 2. [`crate::graph::resolve_slot_mount`] decides the mount;
    ///    incompatibility is fatal ([`LoadoutError::IncompatibleComponent`]).
    /// 3. Grid placements are validated and occupied — collisions and
    ///    overruns fail with [`LoadoutError::PalsCollision`] *before* any
    ///    item is registered, so a failed equip leaves the draft untouched.
    /// 4. The sub-anchor is computed (cell-scaled for grid mounts, composed
    ///    for anchor mounts) and assigned its deterministic layer:
    ///    [`ADAPTER_LAYER`], [`PRIMARY_LAYER`], or [`CHILD_LAYER`].
    /// 5. Required adapters are auto-registered as mount items tied to this
    ///    mount (and garbage-collected by [`LoadoutDraft::unequip_slot_item`]).
    ///
    /// Returns the newly created [`MountedItem`] (adapters receive earlier
    /// mount ids than the component they serve, mirroring physical install
    /// order).
    pub fn equip_component(
        &mut self,
        catalog: &Catalog,
        slot_id: &str,
        component_sku: &str,
        pals_pos: Option<(u8, u8)>,
    ) -> Result<MountedItem, LoadoutError> {
        let component = catalog
            .component(component_sku)
            .cloned()
            .ok_or_else(|| LoadoutError::UnknownComponent {
                sku: component_sku.to_owned(),
            })?;

        // 1. Resolve and snapshot the target slot (interface, anchor, grid).
        let target = SlotTarget::parse(slot_id)?;
        let (slot_snapshot, parent_anchor) = match &target {
            SlotTarget::Base { slot_id } => {
                let slot = self.platform.slot(slot_id).ok_or_else(|| {
                    LoadoutError::UnknownSlot {
                        slot_id: slot_id.clone(),
                    }
                })?;
                (slot.clone(), None)
            }
            SlotTarget::Child {
                parent_mount_id,
                child_slot_id,
                path,
            } => {
                let host = self
                    .mounted
                    .iter()
                    .find(|item| &item.mount_id == parent_mount_id)
                    .ok_or_else(|| LoadoutError::UnknownSlot {
                        slot_id: path.clone(),
                    })?;
                let slot = host
                    .child_slots
                    .iter()
                    .find(|slot| &slot.id == child_slot_id)
                    .ok_or_else(|| LoadoutError::UnknownSlot {
                        slot_id: path.clone(),
                    })?;
                (slot.clone(), Some(host.anchor.clone()))
            }
        };
        let path = target.path().to_owned();
        // Canvas box of the slot itself: adapter hardware and anchor-mounted
        // children position against it.
        let slot_canvas = match &parent_anchor {
            None => slot_snapshot.anchor.clone(),
            Some(parent) => parent.compose(&slot_snapshot.anchor),
        };

        // 2. Compatibility.
        let required_adapters =
            match resolve_slot_mount(&slot_snapshot, &component, &catalog.adapter_bridges) {
                CompatibilityResult::NativeFit { .. } => Vec::new(),
                CompatibilityResult::RequiresAdapter { required_adapters, .. } => {
                    required_adapters
                }
                CompatibilityResult::Incompatible { reason } => {
                    return Err(LoadoutError::IncompatibleComponent {
                        slot_id: path.clone(),
                        component_sku: component.sku.clone(),
                        reason,
                    });
                }
            };

        // 3. Grid placement requirements.
        let uses_grid = slot_snapshot.pals_grid.is_some() && component.pals_footprint.is_some();
        if uses_grid && pals_pos.is_none() {
            return Err(LoadoutError::MissingPalsPosition {
                slot_id: path.clone(),
                component_sku: component.sku.clone(),
            });
        }

        // 4. Compute the component's canvas anchor and perform the grid
        //    placement. `mount_on_pals` validates before mutating, so a
        //    failed placement leaves the draft completely untouched — no
        //    orphaned adapters, no burned mount ids.
        let layer = match &target {
            SlotTarget::Base { .. } => PRIMARY_LAYER,
            SlotTarget::Child { .. } => CHILD_LAYER,
        };
        let anchor = match (&target, uses_grid) {
            (SlotTarget::Base { slot_id }, true) => {
                let pos = pals_pos.expect("MissingPalsPosition guarded above");
                let slot = self
                    .platform
                    .slot_mut(slot_id)
                    .expect("slot existence checked above");
                let mut anchor = mount_on_pals(slot, &component, pos.0, pos.1).map_err(
                    |source| LoadoutError::PalsCollision {
                        slot_id: path.clone(),
                        source,
                    },
                )?;
                anchor.z_index = layer;
                anchor
            }
            (
                SlotTarget::Child {
                    parent_mount_id,
                    child_slot_id,
                    ..
                },
                true,
            ) => {
                let pos = pals_pos.expect("MissingPalsPosition guarded above");
                // mount_on_pals yields the anchor in the child slot's own
                // coordinate space, i.e. relative to the host component.
                let relative = {
                    let slot = child_slot_mut(&mut self.mounted, parent_mount_id, child_slot_id)
                        .expect("child slot existence checked above");
                    mount_on_pals(slot, &component, pos.0, pos.1).map_err(|source| {
                        LoadoutError::PalsCollision {
                            slot_id: path.clone(),
                            source,
                        }
                    })?
                };
                let parent = parent_anchor.expect("child mounts carry a parent anchor");
                let mut anchor = parent.compose(&relative);
                anchor.z_index = layer;
                anchor
            }
            (_, false) => {
                let mut anchor = slot_canvas.compose(&component.anchor);
                anchor.z_index = layer;
                anchor
            }
        };

        // 5. Mount ids: adapters first, mirroring physical install order.
        //    Allocated only after every check above passed, so failed equips
        //    burn no ids: successful mounts are numbered contiguously.
        let adapter_mount_ids: Vec<String> = (0..required_adapters.len())
            .map(|_| self.next_mount_id())
            .collect();
        let component_mount_id = self.next_mount_id();

        // 6. Auto-register the adapter instances.
        for (adapter, mount_id) in required_adapters.iter().zip(&adapter_mount_ids) {
            let mut anchor = slot_canvas.compose(&adapter.anchor);
            anchor.z_index = ADAPTER_LAYER;
            self.mounted.push(MountedItem {
                mount_id: mount_id.clone(),
                sku: adapter.sku.clone(),
                name: adapter.name.clone(),
                is_adapter: true,
                slot_id: path.clone(),
                pals_position: None,
                pals_footprint: None,
                required_adapters: Vec::new(),
                serves_mount: Some(component_mount_id.clone()),
                anchor,
                z_index: ADAPTER_LAYER,
                msrp_cents: adapter.msrp_cents,
                vendor_offers: adapter.vendor_offers.clone(),
                child_slots: Vec::new(),
                child_mounts: Vec::new(),
            });
        }

        // 7. Register the component.
        let item = MountedItem {
            mount_id: component_mount_id.clone(),
            sku: component.sku.clone(),
            name: component.name.clone(),
            is_adapter: false,
            slot_id: path.clone(),
            pals_position: if uses_grid { pals_pos } else { None },
            pals_footprint: component.pals_footprint,
            required_adapters: required_adapters
                .iter()
                .map(|adapter| adapter.sku.clone())
                .collect(),
            serves_mount: None,
            anchor,
            z_index: layer,
            msrp_cents: component.msrp_cents,
            vendor_offers: component.vendor_offers.clone(),
            child_slots: component.child_slots.clone(),
            child_mounts: Vec::new(),
        };
        self.mounted.push(item.clone());

        // 8. Link the child mount to its host item.
        if let SlotTarget::Child { parent_mount_id, .. } = &target {
            if let Some(host) = self
                .mounted
                .iter_mut()
                .find(|item| &item.mount_id == parent_mount_id)
            {
                host.child_mounts.push(component_mount_id.clone());
            }
        }

        // 9. Advise when several items stack on one non-grid slot.
        if !uses_grid {
            let stacked = self
                .mounted
                .iter()
                .filter(|item| !item.is_adapter && item.slot_id == path)
                .count();
            if stacked > 1 {
                self.warnings.push(format!(
                    "slot `{path}` carries {stacked} items; verify only one physically attaches"
                ));
            }
        }

        Ok(item)
    }

    /// Unequip a mounted component by mount id.
    ///
    /// Cascades in this order:
    /// 1. Frees the occupied PALS cells of the item (and of everything in its
    ///    subtree) in their host grids.
    /// 2. Removes every item mounted onto this component's `child_slots`,
    ///    transitively.
    /// 3. Garbage-collects every adapter instance that was auto-registered
    ///    for the removed mounts.
    pub fn unequip_slot_item(&mut self, mount_id: &str) -> Result<(), LoadoutError> {
        let index = self
            .mounted
            .iter()
            .position(|item| item.mount_id == mount_id)
            .ok_or_else(|| LoadoutError::UnknownMount {
                mount_id: mount_id.to_owned(),
            })?;
        if self.mounted[index].is_adapter {
            return Err(LoadoutError::AutoManagedAdapter {
                mount_id: mount_id.to_owned(),
                sku: self.mounted[index].sku.clone(),
            });
        }

        // The removed subtree: the item plus everything on its child slots.
        let mut subtree: Vec<String> = Vec::new();
        collect_subtree(&self.mounted, mount_id, &mut subtree);

        // Adapter instances serving only removed mounts.
        let adapter_ids: Vec<String> = self
            .mounted
            .iter()
            .filter(|item| {
                item.is_adapter
                    && item
                        .serves_mount
                        .as_ref()
                        .is_some_and(|served| subtree.iter().any(|id| id == served))
            })
            .map(|item| item.mount_id.clone())
            .collect();

        // Free PALS cells while every host grid still exists.
        let removed: Vec<String> = subtree.iter().chain(adapter_ids.iter()).cloned().collect();
        for id in &removed {
            let placement = self.mounted.iter().find(|item| &item.mount_id == id).and_then(
                |item| {
                    item.pals_position.zip(item.pals_footprint).map(
                        |((col, row), footprint)| {
                            (
                                item.slot_id.clone(),
                                col,
                                row,
                                footprint.cols,
                                footprint.rows,
                            )
                        },
                    )
                },
            );
            if let Some((slot_path, col, row, cols, rows)) = placement {
                self.release_cells(&slot_path, col, row, cols, rows);
            }
        }

        // Unlink the root from its surviving host, if it was a child mount.
        let root_slot = self.mounted[index].slot_id.clone();
        if let Some((parent_mount_id, _)) = root_slot.split_once('.') {
            if !removed.iter().any(|id| id == parent_mount_id) {
                if let Some(host) = self
                    .mounted
                    .iter_mut()
                    .find(|item| item.mount_id == parent_mount_id)
                {
                    host.child_mounts.retain(|child| child != mount_id);
                }
            }
        }

        self.mounted
            .retain(|item| !removed.contains(&item.mount_id));
        Ok(())
    }

    /// Snapshot the draft into a [`LoadoutSummary`]: the ordered render
    /// stack, the aggregated BOM, and accumulated warnings.
    ///
    /// The summary reports `valid: true` with no errors; API-level evaluation
    /// merges its placement errors in, since the draft itself only ever
    /// contains successful equips.
    pub fn summary(&self) -> LoadoutSummary {
        let mut layers = vec![Layer2D {
            mount_id: "platform".to_owned(),
            sku: self.platform.sku.clone(),
            name: self.platform.name.clone(),
            kind: LayerKind::Platform,
            z_index: PLATFORM_LAYER,
            anchor: AnchorBox2D::new(0.0, 0.0, 100.0, 100.0, PLATFORM_LAYER),
            slot_id: String::new(),
        }];
        let mut items: Vec<&MountedItem> = self.mounted.iter().collect();
        items.sort_by_key(|item| item.z_index); // stable: ties keep mount order
        layers.extend(items.into_iter().map(|item| Layer2D {
            mount_id: item.mount_id.clone(),
            sku: item.sku.clone(),
            name: item.name.clone(),
            kind: if item.is_adapter {
                LayerKind::Adapter
            } else {
                LayerKind::Component
            },
            z_index: item.z_index,
            anchor: item.anchor.clone(),
            slot_id: item.slot_id.clone(),
        }));

        // BOM in first-mount order, aggregated per SKU. Pricing fields are
        // snapshotted per unit at equip time; totals weight them by quantity
        // and include the platform itself.
        let mut bom: Vec<BomLineItem> = Vec::new();
        for item in &self.mounted {
            match bom.iter_mut().find(|line| line.sku == item.sku) {
                Some(line) => line.quantity += 1,
                None => bom.push(BomLineItem {
                    sku: item.sku.clone(),
                    name: item.name.clone(),
                    quantity: 1,
                    is_adapter: item.is_adapter,
                    msrp_cents: item.msrp_cents,
                    best_offer: best_offer(&item.vendor_offers).cloned(),
                    savings_cents: savings_cents(item.msrp_cents, &item.vendor_offers),
                }),
            }
        }

        // Cost rollup: the platform line plus every BOM line, qty-weighted.
        // Saturating arithmetic keeps totals well-defined even on absurd
        // quantities; prices are u32 cents so this never loses precision.
        let mut total_msrp_cents = self.platform.msrp_cents;
        let mut total_best_price_cents =
            effective_price_cents(self.platform.msrp_cents, &self.platform.vendor_offers);
        for line in &bom {
            total_msrp_cents = total_msrp_cents.saturating_add(line.msrp_cents.saturating_mul(line.quantity));
            let unit_best = line
                .best_offer
                .as_ref()
                .map_or(line.msrp_cents, |offer| offer.price_cents);
            total_best_price_cents =
                total_best_price_cents.saturating_add(unit_best.saturating_mul(line.quantity));
        }
        let total_savings_cents = total_msrp_cents.saturating_sub(total_best_price_cents);

        LoadoutSummary {
            platform_sku: self.platform.sku.clone(),
            platform_name: self.platform.name.clone(),
            valid: true,
            layers_2d: layers,
            bom,
            total_msrp_cents,
            total_best_price_cents,
            total_savings_cents,
            warnings: self.warnings.clone(),
            errors: Vec::new(),
        }
    }

    /// Release the cells of a footprint placed at `slot_path` (a platform slot
    /// id or a `<mount_id>.<child_slot_id>` path).
    fn release_cells(&mut self, slot_path: &str, col: u8, row: u8, cols: u8, rows: u8) {
        match slot_path.split_once('.') {
            None => {
                if let Some(grid) = self
                    .platform
                    .slot_mut(slot_path)
                    .and_then(|slot| slot.pals_grid.as_mut())
                {
                    grid.release(col, row, cols, rows);
                }
            }
            Some((parent_mount_id, child_slot_id)) => {
                let Some(host) = self
                    .mounted
                    .iter_mut()
                    .find(|item| item.mount_id == parent_mount_id)
                else {
                    return;
                };
                if let Some(grid) = host
                    .child_slots
                    .iter_mut()
                    .find(|slot| slot.id == child_slot_id)
                    .and_then(|slot| slot.pals_grid.as_mut())
                {
                    grid.release(col, row, cols, rows);
                }
            }
        }
    }

    /// Allocate the next deterministic mount id (`"m1"`, `"m2"`, ...). Ids are
    /// never reused within a draft.
    fn next_mount_id(&mut self) -> String {
        self.mount_seq += 1;
        format!("m{}", self.mount_seq)
    }
}

/// Where an equip targets: a platform slot or a child slot of a mounted item.
#[derive(Debug, Clone, PartialEq)]
enum SlotTarget {
    /// A slot on the base platform, addressed by its slot id.
    Base {
        /// Platform slot id.
        slot_id: String,
    },
    /// A child slot exposed by a mounted item, addressed as
    /// `<mount_id>.<child_slot_id>`.
    Child {
        /// Mount id of the hosting item.
        parent_mount_id: String,
        /// Child slot id on the hosting item.
        child_slot_id: String,
        /// Full path (error reporting and `MountedItem::slot_id`).
        path: String,
    },
}

impl SlotTarget {
    /// Parse a slot address. Platform slot ids never contain `.`; a single
    /// dot separates a host mount id from its child slot id.
    fn parse(slot_id: &str) -> Result<Self, LoadoutError> {
        match slot_id.split_once('.') {
            None => Ok(SlotTarget::Base {
                slot_id: slot_id.to_owned(),
            }),
            Some((parent_mount_id, child_slot_id))
                if !parent_mount_id.is_empty() && !child_slot_id.is_empty() =>
            {
                Ok(SlotTarget::Child {
                    parent_mount_id: parent_mount_id.to_owned(),
                    child_slot_id: child_slot_id.to_owned(),
                    path: slot_id.to_owned(),
                })
            }
            Some(_) => Err(LoadoutError::UnknownSlot {
                slot_id: slot_id.to_owned(),
            }),
        }
    }

    /// The full slot path used in `MountedItem::slot_id`.
    fn path(&self) -> &str {
        match self {
            SlotTarget::Base { slot_id } => slot_id,
            SlotTarget::Child { path, .. } => path,
        }
    }
}

/// Mutable access to a child slot of a mounted item, by host mount id and
/// child slot id.
fn child_slot_mut<'a>(
    mounted: &'a mut [MountedItem],
    parent_mount_id: &str,
    child_slot_id: &str,
) -> Option<&'a mut MountSlot> {
    mounted
        .iter_mut()
        .find(|item| item.mount_id == parent_mount_id)?
        .child_slots
        .iter_mut()
        .find(|slot| slot.id == child_slot_id)
}

/// Depth-first collection of `mount_id` plus everything mounted on its child
/// slots, transitively.
fn collect_subtree(mounted: &[MountedItem], mount_id: &str, out: &mut Vec<String>) {
    out.push(mount_id.to_owned());
    let Some(item) = mounted.iter().find(|item| item.mount_id == mount_id) else {
        return;
    };
    let children = item.child_mounts.clone();
    for child in children {
        collect_subtree(mounted, &child, out);
    }
}
