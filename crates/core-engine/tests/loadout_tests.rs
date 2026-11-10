//! Spec-03 acceptance tests: stateful loadout assembly.
//!
//! Covers adapter auto-injection and deterministic render layers, live
//! PALS/MOLLE grid collision tracking, unequip garbage collection, and child
//! slots exposed by mounted placards.
//!
//! Fixtures mirror the sample catalog (the Google Sheet exports): the JPC 2.0
//! front flap is a Crye AVS flap (bridged to 1" SwiftClip by the AXL AVP), the
//! AWS SMU Belt carries a full 18x2 outer PALS field, the MK5 placard mounts
//! via 1" SwiftClip and exposes its own 6x3 front PALS field, and the Esstac
//! KYWI double pouch has a 2x2 footprint.

use core_engine::catalog::Catalog;
use core_engine::graph::{
    ComponentKind, ComponentSpec, MountSlot, PalsFootprint, PlatformCategory, PlatformSpec,
};
use core_engine::interfaces::{InterfaceStandard, PlateCut, Size};
use core_engine::loadout::{
    LoadoutDraft, LoadoutError, ADAPTER_LAYER, CHILD_LAYER, PLATFORM_LAYER, PRIMARY_LAYER,
};
use core_engine::spatial::{AnchorBox2D, PalsMatrix, SpatialError};
use core_engine::LayerKind;
use uuid::Uuid;

// --- fixtures ----------------------------------------------------------------

/// Crye JPC 2.0 (from the plate-carriers sheet): front flap is a Crye AVS
/// flap, so 1" SwiftClip placards mount through the AXL AVP adapter.
fn crye_jpc_2() -> PlatformSpec {
    PlatformSpec {
        id: Uuid::new_v4(),
        sku: "crye-precision-jpc-2-0".into(),
        name: "Crye Precision JPC 2.0".into(),
        category: Some(PlatformCategory::PlateCarrier),
        plate_cuts: vec![PlateCut::SapiEsapi, PlateCut::Swimmer],
        sizes: vec![Size::Small, Size::Medium, Size::Large, Size::XLarge],
        slots: vec![
            MountSlot {
                id: "front_placard".into(),
                label: "Front placard field".into(),
                interface: InterfaceStandard::CryeAvsFlap,
                anchor: AnchorBox2D::new(30.0, 18.0, 40.0, 46.0, 10),
                pals_grid: None,
            },
            MountSlot {
                id: "rear_panel".into(),
                label: "Zip-on back panel".into(),
                interface: InterfaceStandard::CryeZip10,
                anchor: AnchorBox2D::new(30.0, 20.0, 40.0, 44.0, 10),
                pals_grid: None,
            },
        ],
    }
}

/// AWS SMU Belt (from the belts sheet): full 18-column 1" PALS outer field.
fn aws_smu_belt() -> PlatformSpec {
    PlatformSpec {
        id: Uuid::new_v4(),
        sku: "aws-smu-belt".into(),
        name: "SMU Belt".into(),
        category: Some(PlatformCategory::Belt),
        plate_cuts: Vec::new(),
        sizes: vec![Size::Small, Size::Medium, Size::Large, Size::XLarge],
        slots: vec![MountSlot {
            id: "belt_outer_pals".into(),
            label: "Outer PALS field".into(),
            interface: InterfaceStandard::StandardPals1In,
            anchor: AnchorBox2D::new(2.0, 40.0, 96.0, 20.0, 5),
            pals_grid: Some(PalsMatrix::new(18, 2)),
        }],
    }
}

/// Spiritus Systems MK5 Placard: 1" SwiftClip mount, exposes a 6x3 front
/// PALS field once mounted. The child-slot anchor is relative to the
/// placard's render anchor.
fn spiritus_mk5_placard() -> ComponentSpec {
    ComponentSpec {
        id: Uuid::new_v4(),
        sku: "spiritus-mk5-placard".into(),
        name: "Spiritus Systems MK5 Placard".into(),
        kind: ComponentKind::Placard,
        interface: InterfaceStandard::SwiftClip1InchQasm,
        anchor: AnchorBox2D::new(0.0, 0.0, 100.0, 100.0, 1),
        pals_footprint: None,
        child_slots: vec![MountSlot {
            id: "placard_front_pals".into(),
            label: "Placard front PALS field".into(),
            interface: InterfaceStandard::StandardPals1In,
            anchor: AnchorBox2D::new(4.0, 10.0, 92.0, 82.0, 4),
            pals_grid: Some(PalsMatrix::new(6, 3)),
        }],
    }
}

/// Esstac KYWI 5.56 Double: 2x2 PALS footprint.
fn esstac_kywi_double() -> ComponentSpec {
    ComponentSpec {
        id: Uuid::new_v4(),
        sku: "esstac-kywi-556-double".into(),
        name: "Esstac KYWI 5.56 Double".into(),
        kind: ComponentKind::BeltItem,
        interface: InterfaceStandard::StandardPals1In,
        anchor: AnchorBox2D::new(0.0, 0.0, 100.0, 100.0, 2),
        pals_footprint: Some(PalsFootprint { cols: 2, rows: 2 }),
        child_slots: Vec::new(),
    }
}

/// AXL AVP (AVS Vertical Placard) adapter: bridges Crye AVS flaps to 1"
/// SwiftClip placards.
fn axl_avp_jpc() -> core_engine::graph::AdapterBridge {
    core_engine::graph::AdapterBridge {
        id: Uuid::new_v4(),
        sku: "axl-avp-jpc".into(),
        name: "AXL AVP (AVS Vertical Placard) Adapter".into(),
        platform_interface: InterfaceStandard::CryeAvsFlap,
        component_interface: InterfaceStandard::SwiftClip1InchQasm,
        // Adapter hardware sits at the top of the slot.
        anchor: AnchorBox2D::new(0.0, 0.0, 100.0, 6.0, 1),
    }
}

/// The fixture catalog: JPC 2.0 + SMU belt, MK5 placard + KYWI pouch, AXL AVP.
fn catalog() -> Catalog {
    Catalog {
        platforms: vec![crye_jpc_2(), aws_smu_belt()],
        components: vec![spiritus_mk5_placard(), esstac_kywi_double()],
        adapter_bridges: vec![axl_avp_jpc()],
    }
}

// --- Spec-03 required tests -----------------------------------------------------

/// Equip `spiritus-mk5-placard` on `crye-precision-jpc-2-0`: the AXL AVP
/// adapter is auto-added to the BOM and z_index layers are ordered
/// Platform (0) < Adapter (10) < Placard (20).
#[test]
fn adapter_auto_injection_and_layer_order() {
    let catalog = catalog();
    let platform = catalog.platform("crye-precision-jpc-2-0").unwrap().clone();
    let mut draft = LoadoutDraft::new(platform);

    let placard = draft
        .equip_component(&catalog, "front_placard", "spiritus-mk5-placard", None)
        .expect("MK5 mounts on the JPC front flap through the AXL AVP");

    // The adapter is registered first (m1), the placard it serves second (m2).
    assert_eq!(placard.mount_id, "m2");
    assert_eq!(placard.required_adapters, vec!["axl-avp-jpc"]);
    assert_eq!(placard.z_index, PRIMARY_LAYER);

    let summary = draft.summary();
    assert!(summary.valid);
    assert!(summary.warnings.is_empty());

    // Render stack: platform, adapter, placard — strictly increasing layers.
    assert_eq!(summary.layers_2d.len(), 3);
    assert_eq!(summary.layers_2d[0].kind, LayerKind::Platform);
    assert_eq!(summary.layers_2d[0].z_index, PLATFORM_LAYER);
    assert_eq!(summary.layers_2d[1].kind, LayerKind::Adapter);
    assert_eq!(summary.layers_2d[1].sku, "axl-avp-jpc");
    assert_eq!(summary.layers_2d[1].z_index, ADAPTER_LAYER);
    assert_eq!(summary.layers_2d[2].kind, LayerKind::Component);
    assert_eq!(summary.layers_2d[2].sku, "spiritus-mk5-placard");
    assert_eq!(summary.layers_2d[2].z_index, PRIMARY_LAYER);
    assert!(
        summary
            .layers_2d
            .windows(2)
            .all(|pair| pair[0].z_index < pair[1].z_index),
        "layers must be ordered by z_index"
    );

    // The adapter appears in the BOM, flagged as adapter hardware.
    let adapter_line = summary
        .bom
        .iter()
        .find(|line| line.sku == "axl-avp-jpc")
        .expect("adapter is auto-added to the BOM");
    assert_eq!(adapter_line.quantity, 1);
    assert!(adapter_line.is_adapter);
    let placard_line = summary
        .bom
        .iter()
        .find(|line| line.sku == "spiritus-mk5-placard")
        .expect("placard is in the BOM");
    assert_eq!(placard_line.quantity, 1);
    assert!(!placard_line.is_adapter);
}

/// Two `esstac-kywi-556-double` (2 columns wide) on `aws-smu-belt` at col 0
/// then col 1: the second mount fails with `PalsCollision`, then succeeds at
/// col 2. Unequipping the first frees its cells for reuse.
#[test]
fn pals_collision_then_success_at_next_free_column() {
    let catalog = catalog();
    let platform = catalog.platform("aws-smu-belt").unwrap().clone();
    let mut draft = LoadoutDraft::new(platform);

    let first = draft
        .equip_component(
            &catalog,
            "belt_outer_pals",
            "esstac-kywi-556-double",
            Some((0, 0)),
        )
        .expect("first KYWI at column 0");
    assert_eq!(first.mount_id, "m1");
    assert_eq!(first.pals_position, Some((0, 0)));

    // Column 0 + 2-wide footprint occupies columns 0-1, so column 1 overlaps.
    let error = draft
        .equip_component(
            &catalog,
            "belt_outer_pals",
            "esstac-kywi-556-double",
            Some((1, 0)),
        )
        .expect_err("column 1 overlaps the first pouch");
    match error {
        LoadoutError::PalsCollision {
            slot_id,
            source: SpatialError::AlreadyOccupied { .. },
        } => assert_eq!(slot_id, "belt_outer_pals"),
        other => panic!("expected PalsCollision/AlreadyOccupied, got {other:?}"),
    }

    // A failed equip must not have registered anything (and burned no id).
    assert_eq!(draft.mounted().len(), 1);

    // Column 2 is clear of the 2-wide first pouch.
    let second = draft
        .equip_component(
            &catalog,
            "belt_outer_pals",
            "esstac-kywi-556-double",
            Some((2, 0)),
        )
        .expect("second KYWI at column 2");
    assert_eq!(second.mount_id, "m2");
    assert_eq!(second.pals_position, Some((2, 0)));

    let summary = draft.summary();
    assert!(summary.valid);
    let kywi = summary
        .bom
        .iter()
        .find(|line| line.sku == "esstac-kywi-556-double")
        .expect("KYWI in the BOM");
    assert_eq!(kywi.quantity, 2);

    // Unequipping the first pouch frees its cells for reuse (the second
    // pouch still holds columns 2-3).
    draft.unequip_slot_item("m1").expect("unequip first pouch");
    let replacement = draft
        .equip_component(
            &catalog,
            "belt_outer_pals",
            "esstac-kywi-556-double",
            Some((0, 0)),
        )
        .expect("column 0 is free again after unequip");
    assert_eq!(replacement.pals_position, Some((0, 0)));
}

/// Unequip `spiritus-mk5-placard`: the `axl-avp-jpc` adapter is automatically
/// removed from the BOM (adapters are auto-managed and cannot be unequipped
/// directly).
#[test]
fn unequip_placard_garbage_collects_adapter() {
    let catalog = catalog();
    let platform = catalog.platform("crye-precision-jpc-2-0").unwrap().clone();
    let mut draft = LoadoutDraft::new(platform);

    let placard = draft
        .equip_component(&catalog, "front_placard", "spiritus-mk5-placard", None)
        .expect("MK5 mounts");
    let adapter_mount_id = draft
        .mounted()
        .iter()
        .find(|item| item.is_adapter)
        .map(|item| item.mount_id.clone())
        .expect("adapter registered");
    assert_eq!(adapter_mount_id, "m1");

    // Adapters are auto-managed: direct unequip is refused.
    match draft.unequip_slot_item("m1") {
        Err(LoadoutError::AutoManagedAdapter { mount_id, sku }) => {
            assert_eq!(mount_id, "m1");
            assert_eq!(sku, "axl-avp-jpc");
        }
        other => panic!("expected AutoManagedAdapter, got {other:?}"),
    }

    // Unequipping the component garbage-collects its adapter.
    draft
        .unequip_slot_item(&placard.mount_id)
        .expect("unequip placard");
    assert!(draft.mounted().is_empty());

    let summary = draft.summary();
    assert!(summary.bom.is_empty());
    assert_eq!(summary.layers_2d.len(), 1, "only the platform layer remains");
    assert_eq!(summary.layers_2d[0].kind, LayerKind::Platform);
}

// --- child-slot coverage ----------------------------------------------------------

/// A mounted placard exposes its `placard_front_pals` child slot, addressable
/// as `<mount_id>.placard_front_pals`; items mounted there render on the
/// child layer (30) and cascade away when the host placard is unequipped.
#[test]
fn child_slots_mount_and_cascade_on_unequip() {
    let catalog = catalog();
    let platform = catalog.platform("crye-precision-jpc-2-0").unwrap().clone();
    let mut draft = LoadoutDraft::new(platform);

    let placard = draft
        .equip_component(&catalog, "front_placard", "spiritus-mk5-placard", None)
        .expect("MK5 mounts");
    assert_eq!(placard.mount_id, "m2");

    // The mounted placard exposes its front PALS field as a child slot.
    let child_slot_path = format!("{}.placard_front_pals", placard.mount_id);
    let pouch = draft
        .equip_component(&catalog, &child_slot_path, "esstac-kywi-556-double", Some((0, 0)))
        .expect("KYWI mounts on the placard's front field");
    assert_eq!(pouch.mount_id, "m3");
    assert_eq!(pouch.slot_id, child_slot_path);
    assert_eq!(pouch.z_index, CHILD_LAYER);

    // The child anchor composes: placard canvas box -> slot-relative anchor
    // -> cell offset. (JPC front field at (30, 18, 40, 46); placard fills it;
    // child field at (4, 10) of 92x82 within the placard.)
    assert!((pouch.anchor.x_pct - 31.6).abs() < 1e-3, "got {}", pouch.anchor.x_pct);
    assert!((pouch.anchor.y_pct - 22.6).abs() < 1e-3, "got {}", pouch.anchor.y_pct);

    // The host tracks its child mount.
    assert_eq!(
        draft.mounted_item("m2").unwrap().child_mounts,
        vec!["m3".to_owned()]
    );

    // The child slot's grid tracks occupancy live.
    let _ = draft
        .equip_component(&catalog, &child_slot_path, "esstac-kywi-556-double", Some((0, 0)))
        .expect_err("cell (0, 0) is occupied");

    // Unequipping the placard cascades to its child mounts and adapters.
    draft
        .unequip_slot_item(&placard.mount_id)
        .expect("unequip placard");
    assert!(draft.mounted().is_empty());
    let summary = draft.summary();
    assert!(summary.bom.is_empty());
    assert_eq!(summary.layers_2d.len(), 1);
}
