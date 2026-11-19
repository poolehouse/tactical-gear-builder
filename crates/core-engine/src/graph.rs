//! The compatibility graph: platforms, mount slots, kit sections, components,
//! adapters, and the deterministic mount resolver.

use crate::interfaces::{InterfaceFamily, InterfaceStandard, PlateCut, Size};
use crate::pricing::{Colorway, VendorOffer};
use crate::spatial::{AnchorBox2D, PalsMatrix, SpatialError};
pub use crate::spatial::{MaterialCoverage, PalsFootprint, SectionFrame3D};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Discrete physical face or zone of a tactical platform, used for multi-face
/// 2D section switching and 3D mesh plane projection.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum KitSection {
    /// Front torso / placard face.
    Front,
    /// Rear torso / back-panel face.
    Rear,
    /// Left cummerbund / wing face.
    LeftSide,
    /// Right cummerbund / wing face.
    RightSide,
    /// Sub-load / dangler / buttpack zone.
    SubLoad,
    /// Waist / battle belt ring.
    Belt,
}

impl KitSection {
    /// Infer the canonical `KitSection` from a slot identifier.
    pub fn from_slot_id(slot_id: &str) -> Self {
        let bare = slot_id.rsplit_once('.').map_or(slot_id, |(_, child)| child);
        if bare.contains("rear") || bare.contains("back") || bare.contains("panel_tool") {
            Self::Rear
        } else if bare.contains("left") {
            Self::LeftSide
        } else if bare.contains("right") {
            Self::RightSide
        } else if bare.contains("dangler") || bare.contains("buttpack") || bare.contains("sub_load") {
            Self::SubLoad
        } else if bare.contains("belt") || bare.contains("holster") {
            Self::Belt
        } else {
            Self::Front
        }
    }

    /// Flat-lay top-down coordinate frame (`normal`, `origin_3d`, `up_3d`):
    /// inner platebag faces the floor (`-Z`), outer MOLLE webbing faces the
    /// ceiling (`+Z`), and the rear platebag sits above (`+Y`) the front
    /// platebag (`-Y`), joined through the shoulder straps.
    pub const fn default_frame_3d(self) -> SectionFrame3D {
        match self {
            Self::Rear => SectionFrame3D {
                normal: [0.0, 0.0, 1.0],
                origin_3d: [0.0, 0.34, 0.06],
                up_3d: [0.0, 1.0, 0.0],
            },
            Self::Front => SectionFrame3D {
                normal: [0.0, 0.0, 1.0],
                origin_3d: [0.0, -0.14, 0.06],
                up_3d: [0.0, -1.0, 0.0],
            },
            Self::LeftSide => SectionFrame3D {
                normal: [0.0, 0.0, 1.0],
                origin_3d: [-0.36, -0.14, 0.04],
                up_3d: [0.0, -1.0, 0.0],
            },
            Self::RightSide => SectionFrame3D {
                normal: [0.0, 0.0, 1.0],
                origin_3d: [0.36, -0.14, 0.04],
                up_3d: [0.0, -1.0, 0.0],
            },
            Self::SubLoad => SectionFrame3D {
                normal: [0.0, 0.0, 1.0],
                origin_3d: [0.0, -0.44, 0.05],
                up_3d: [0.0, -1.0, 0.0],
            },
            Self::Belt => SectionFrame3D {
                normal: [0.0, 0.0, 1.0],
                origin_3d: [0.0, -0.58, 0.04],
                up_3d: [0.0, -1.0, 0.0],
            },
        }
    }
}

/// A physical mount point on a platform (a placard field, zipper track,
/// cummerbund face, or PALS/MOLLE grid).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MountSlot {
    /// Stable machine identifier used to address the slot. Slot addressing
    /// only — never used for compatibility matching.
    pub id: String,
    /// Human-readable label for the builder UI.
    pub label: String,
    /// The attachment standard this slot natively exposes.
    pub interface: InterfaceStandard,
    /// Canvas anchor of the slot area for the paper-doll renderer.
    pub anchor: AnchorBox2D,
    /// Occupancy grid, present when the slot exposes a PALS/MOLLE field.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pals_grid: Option<PalsMatrix>,
}

impl MountSlot {
    /// The `KitSection` this slot belongs to, derived deterministically from
    /// its slot identifier.
    pub fn section(&self) -> KitSection {
        KitSection::from_slot_id(&self.id)
    }

    /// The 3D-ready coordinate frame for this slot's section plane.
    pub fn frame_3d(&self) -> SectionFrame3D {
        self.section().default_frame_3d()
    }
}

/// Broad category of a load-bearing platform, used for catalog filtering and
/// renderer grouping (never for interface matching).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PlatformCategory {
    /// Armor plate carrier.
    PlateCarrier,
    /// Gun / battle belt.
    Belt,
    /// Chest rig.
    ChestRig,
    /// Load-bearing equipment (harness plus sustainment field gear).
    Lbe,
}

/// A load-bearing platform (plate carrier, chest rig, or belt).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PlatformSpec {
    /// Unique platform identifier.
    pub id: Uuid,
    /// Stable SKU slug used to address this platform in APIs and loadouts.
    #[serde(default)]
    pub sku: String,
    /// Product name (display only — never used for compatibility matching).
    pub name: String,
    /// Platform category (absent in legacy seeds).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub category: Option<PlatformCategory>,
    /// Armor plate cuts the platform accepts.
    pub plate_cuts: Vec<PlateCut>,
    /// Sizes the platform is offered in.
    pub sizes: Vec<Size>,
    /// Mount points the platform exposes.
    pub slots: Vec<MountSlot>,
    /// Manufacturer's suggested retail price, US cents.
    pub msrp_cents: u32,
    /// Competitive vendor listings (see [`crate::pricing::best_offer`]).
    pub vendor_offers: Vec<VendorOffer>,
    /// Colorways the platform is offered in.
    pub allowed_colorways: Vec<Colorway>,
    /// Colorway the platform ships in by default.
    pub default_colorway: Colorway,
}

impl PlatformSpec {
    /// Look up a slot by id.
    pub fn slot(&self, slot_id: &str) -> Option<&MountSlot> {
        self.slots.iter().find(|slot| slot.id == slot_id)
    }

    /// Look up a slot by id for mutation (e.g. PALS placement).
    pub fn slot_mut(&mut self, slot_id: &str) -> Option<&mut MountSlot> {
        self.slots.iter_mut().find(|slot| slot.id == slot_id)
    }

    /// Whether the platform accepts the given armor plate cut.
    pub fn supports_plate_cut(&self, cut: PlateCut) -> bool {
        self.plate_cuts.contains(&cut)
    }
}

/// Broad category of a mountable component (for renderer grouping and future
/// rules; not used for interface matching).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ComponentKind {
    /// Front placard.
    Placard,
    /// Zip-on or MOLLE back panel.
    BackPanel,
    /// Front cummerbund pouch.
    CummerbundFrontPouch,
    /// Rear cummerbund pouch.
    CummerbundRearPouch,
    /// Belt-mounted or general PALS item.
    BeltItem,
}

/// A typed operational capability a component provides once mounted.
///
/// Role and regulation checks match exclusively on these tags — never on
/// product names, SKUs, or free text (R3). `RifleMagCapacity` carries the
/// number of primary rifle magazines the item holds; the rest are
/// present/absent capabilities.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum CapabilityTag {
    /// Holds `u8` primary rifle magazines (pouch cells, placard inserts).
    RifleMagCapacity(u8),
    /// Medical IFAK / trauma kit storage.
    MedicalIfak,
    /// Comms routing or PTT housing.
    CommsRoutingOrPtt,
    /// Admin / ATAK device mount or utility storage.
    AdminAtakUtility,
    /// Breaching tools or ordnance / bang pouch storage.
    BreachingOrOrdnance,
    /// Hydration or general sustainment capacity.
    HydrationSustainment,
    /// Holster / weapon retention mount.
    HolsterRetention,
    /// Contributes to a prone-friendly, low-profile front profile.
    ProneLowProfile,
}

/// A mountable component (placard, pouch, back panel, tool, munition, radio...).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ComponentSpec {
    /// Unique component identifier.
    pub id: Uuid,
    /// Stable SKU slug used to address this component in APIs and loadouts.
    #[serde(default)]
    pub sku: String,
    /// Product name (display only — never used for compatibility matching).
    pub name: String,
    /// Category of the component.
    pub kind: ComponentKind,
    /// The attachment standard the component mates with.
    pub interface: InterfaceStandard,
    /// Render anchor, expressed relative to the mount slot's anchor.
    pub anchor: AnchorBox2D,
    /// PALS attachment footprint (`1x1` webbing cuts the straps weave into).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pals_footprint: Option<PalsFootprint>,
    /// Physical material coverage footprint when the item's body/fabric covers
    /// more `1x1` cells than its `pals_footprint` attachment straps.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub material_coverage: Option<MaterialCoverage>,
    /// Slots this component exposes once mounted (e.g. a placard's front
    /// PALS field). Slot anchors are relative to this component's render
    /// [`anchor`](ComponentSpec::anchor).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub child_slots: Vec<MountSlot>,
    /// Manufacturer's suggested retail price, US cents.
    pub msrp_cents: u32,
    /// Competitive vendor listings (see [`crate::pricing::best_offer`]).
    pub vendor_offers: Vec<VendorOffer>,
    /// Colorways the component is offered in.
    pub allowed_colorways: Vec<Colorway>,
    /// Colorway the component ships in by default.
    pub default_colorway: Colorway,
    /// Typed operational capabilities (role/regulation checks match on
    /// these — never names or SKUs).
    pub capabilities: Vec<CapabilityTag>,
}

/// A single-hop aftermarket adapter that physically bridges one platform
/// interface to another (e.g. an AXL AVP bridging a Crye AVS flap to 1"
/// SwiftClip placards).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AdapterBridge {
    /// Unique adapter identifier.
    pub id: Uuid,
    /// Stable SKU slug used to address this adapter in BOMs and APIs.
    #[serde(default)]
    pub sku: String,
    /// Product name (display only — never used for compatibility matching).
    pub name: String,
    /// Interface on the platform side that the adapter attaches to.
    pub platform_interface: InterfaceStandard,
    /// Interface the adapter exposes to the component.
    pub component_interface: InterfaceStandard,
    /// Canvas area consumed by the adapter hardware, relative to the slot
    /// anchor.
    pub anchor: AnchorBox2D,
    /// Manufacturer's suggested retail price, US cents.
    pub msrp_cents: u32,
    /// Competitive vendor listings (see [`crate::pricing::best_offer`]).
    pub vendor_offers: Vec<VendorOffer>,
    /// Colorways the adapter is offered in.
    pub allowed_colorways: Vec<Colorway>,
    /// Colorway the adapter ships in by default.
    pub default_colorway: Colorway,
}

impl AdapterBridge {
    /// Whether this adapter bridges `slot_interface` to `component_interface`.
    pub fn bridges(
        &self,
        slot_interface: InterfaceStandard,
        component_interface: InterfaceStandard,
    ) -> bool {
        self.platform_interface == slot_interface
            && self.component_interface == component_interface
    }
}

/// Why a component cannot mount onto a platform.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum IncompatibilityReason {
    /// No slot with the requested id exists on the platform.
    UnknownSlot {
        /// The requested slot id.
        slot_id: String,
    },
    /// Slot and component belong to different attachment families, so no
    /// single physical adapter can ever bridge them.
    InterfaceFamilyMismatch {
        /// The standard exposed by the slot.
        slot_interface: InterfaceStandard,
        /// The standard the component mates with.
        component_interface: InterfaceStandard,
    },
    /// Both sides are zipper tracks but of different proprietary gauge —
    /// there is no physical zipper-track adapter on the market.
    ZipperGaugeMismatch {
        /// The standard exposed by the slot.
        slot_interface: InterfaceStandard,
        /// The standard the component mates with.
        component_interface: InterfaceStandard,
    },
    /// Same family, but no registered single-hop adapter bridges the two
    /// standards.
    NoAdapterFound {
        /// The standard exposed by the slot.
        slot_interface: InterfaceStandard,
        /// The standard the component mates with.
        component_interface: InterfaceStandard,
    },
}

/// The outcome of a mount resolution.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum CompatibilityResult {
    /// The component mounts directly onto the slot.
    NativeFit {
        /// Canvas anchor for the renderer.
        anchor: AnchorBox2D,
    },
    /// The component mounts through exactly one adapter (single-hop).
    RequiresAdapter {
        /// The single adapter that must be installed first.
        required_adapters: Vec<AdapterBridge>,
        /// Canvas anchor for the renderer, including adapter hardware.
        anchor: AnchorBox2D,
    },
    /// No valid mount exists.
    Incompatible {
        /// Why the mount failed.
        reason: IncompatibilityReason,
    },
}

impl CompatibilityResult {
    /// Whether the component can mount at all.
    pub fn is_compatible(&self) -> bool {
        !matches!(self, Self::Incompatible { .. })
    }

    /// The resolved canvas anchor, if the mount is possible.
    pub fn anchor(&self) -> Option<&AnchorBox2D> {
        match self {
            Self::NativeFit { anchor } | Self::RequiresAdapter { anchor, .. } => Some(anchor),
            Self::Incompatible { .. } => None,
        }
    }
}

/// Resolve whether `component` can mount onto `platform`'s slot `slot_id`,
/// considering at most one adapter hop.
pub fn resolve_mount(
    platform: &PlatformSpec,
    slot_id: &str,
    component: &ComponentSpec,
    registry_adapters: &[AdapterBridge],
) -> CompatibilityResult {
    let Some(slot) = platform.slot(slot_id) else {
        return CompatibilityResult::Incompatible {
            reason: IncompatibilityReason::UnknownSlot {
                slot_id: slot_id.to_owned(),
            },
        };
    };
    resolve_slot_mount(slot, component, registry_adapters)
}

/// Resolve whether `component` can mount onto one specific `slot`,
/// considering at most one adapter hop.
pub fn resolve_slot_mount(
    slot: &MountSlot,
    component: &ComponentSpec,
    registry_adapters: &[AdapterBridge],
) -> CompatibilityResult {
    let slot_interface = slot.interface;
    let component_interface = component.interface;

    // 1. The slot must exist and be of the same InterfaceFamily as the
    //    component — OR, when the slot exposes a physical 1" PALS webbing
    //    grid (`pals_grid.is_some()`), a StandardPals1In component mounts
    //    natively onto its PALS webbing cuts.
    if slot_interface == component_interface
        || (slot.pals_grid.is_some() && component_interface == InterfaceStandard::StandardPals1In)
    {
        return CompatibilityResult::NativeFit {
            anchor: slot.anchor.compose(&component.anchor),
        };
    }

    if slot_interface.family() != component_interface.family() {
        return CompatibilityResult::Incompatible {
            reason: IncompatibilityReason::InterfaceFamilyMismatch {
                slot_interface,
                component_interface,
            },
        };
    }

    if let Some(adapter) = registry_adapters
        .iter()
        .find(|adapter| adapter.bridges(slot_interface, component_interface))
    {
        return CompatibilityResult::RequiresAdapter {
            required_adapters: vec![adapter.clone()],
            anchor: slot
                .anchor
                .compose(&adapter.anchor)
                .compose(&component.anchor),
        };
    }

    CompatibilityResult::Incompatible {
        reason: classify_mismatch(slot_interface, component_interface),
    }
}

/// Classify a same-family mismatch that no adapter bridges.
fn classify_mismatch(
    slot_interface: InterfaceStandard,
    component_interface: InterfaceStandard,
) -> IncompatibilityReason {
    if slot_interface.is_zipper() && component_interface.is_zipper() {
        IncompatibilityReason::ZipperGaugeMismatch {
            slot_interface,
            component_interface,
        }
    } else {
        IncompatibilityReason::NoAdapterFound {
            slot_interface,
            component_interface,
        }
    }
}

/// Place a PALS-footprinted component onto a slot's grid: mutates occupancy
/// (both attachment cells and any material coverage overhang) and returns the
/// component's canvas anchor.
///
/// The returned `AnchorBox2D` spans the visual `material_coverage` envelope
/// when present (so overhanging tools/pouches render at their true physical
/// coverage size), or the `pals_footprint` when `material_coverage` is `None`.
pub fn mount_on_pals(
    slot: &mut MountSlot,
    component: &ComponentSpec,
    col_idx: u8,
    row_idx: u8,
) -> Result<AnchorBox2D, SpatialError> {
    let slot_id = slot.id.clone();
    let slot_anchor = slot.anchor.clone();
    let footprint = component.pals_footprint.ok_or_else(|| SpatialError::NoPalsFootprint {
        component: component.name.clone(),
    })?;
    let grid = slot.pals_grid.as_mut().ok_or(SpatialError::NoPalsGrid { slot_id })?;

    grid.mount_with_coverage(col_idx, row_idx, footprint, component.material_coverage)?;

    let cell_w = slot_anchor.width_pct / f32::from(grid.cols);
    let cell_h = slot_anchor.height_pct / f32::from(grid.rows);

    Ok(AnchorBox2D {
        x_pct: slot_anchor.x_pct + cell_w * f32::from(col_idx),
        y_pct: slot_anchor.y_pct + cell_h * f32::from(row_idx),
        width_pct: cell_w * f32::from(footprint.cols),
        height_pct: cell_h * f32::from(footprint.rows),
        z_index: slot_anchor.z_index.saturating_add(component.anchor.z_index),
    })
}

/// Convenience re-export so callers can talk about families without importing
/// the interfaces module directly.
pub type Family = InterfaceFamily;
