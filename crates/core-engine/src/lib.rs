//! `core-engine` — the deterministic compatibility core for the tactical gear
//! visual builder.
//!
//! Pure compute, zero I/O. Given a [`graph::PlatformSpec`], a
//! [`graph::ComponentSpec`], and an adapter registry, it decides whether a
//! component mounts natively, requires exactly one aftermarket adapter, or is
//! incompatible — plus all PALS/MOLLE occupancy math and 2D sprite anchor
//! coordinates for the paper-doll renderer.
//!
//! Design rules:
//! - No free-text compatibility matching: every attachment standard is a
//!   strongly typed [`interfaces::InterfaceStandard`] variant.
//! - Adapter bridging is single-hop only (one physical intermediary).
//! - All public data types are `Debug + Clone + PartialEq + Serialize +
//!   Deserialize` so specs can cross the service boundary as JSON.
//! - Every function is deterministic: results depend only on their inputs.

#![forbid(unsafe_code)]
#![warn(missing_docs)]

pub mod catalog;
pub mod codec;
pub mod graph;
pub mod interfaces;
pub mod loadout;
pub mod spatial;

pub use catalog::Catalog;
pub use graph::{
    AdapterBridge, CompatibilityResult, ComponentKind, ComponentSpec, IncompatibilityReason,
    MountSlot, PalsFootprint, PlatformCategory, PlatformSpec, mount_on_pals, resolve_mount,
    resolve_slot_mount,
};
/// Re-export so downstream crates can construct `Uuid` id fields without
/// adding `uuid` as a direct dependency.
pub use uuid;
pub use interfaces::{InterfaceFamily, InterfaceStandard, PlateCut, Size};
pub use loadout::{
    ADAPTER_LAYER, CHILD_LAYER, PLATFORM_LAYER, PRIMARY_LAYER, BomLineItem, Layer2D, LayerKind,
    LoadoutDraft, LoadoutError, LoadoutSummary, MountedItem,
};
pub use spatial::{AnchorBox2D, PalsMatrix, SpatialError};
