//! Spec-01 acceptance tests: deterministic gear compatibility, single-hop
//! aftermarket adapter bridging, and PALS/MOLLE grid occupancy math.
//!
//! Fixtures are grounded in the platform CSV
//! (`~/Documents/Tactical Gear Platforms & Compatibility Database`).

use core_engine::codec::{from_json, to_json};
use core_engine::graph::{
    mount_on_pals, resolve_mount, AdapterBridge, CompatibilityResult, ComponentKind,
    ComponentSpec, IncompatibilityReason, MountSlot, PlatformCategory, PlatformSpec,
};
use core_engine::interfaces::{InterfaceStandard, PlateCut, Size};
use core_engine::spatial::{AnchorBox2D, PalsMatrix, SpatialError};
use uuid::Uuid;

// --- fixtures ----------------------------------------------------------------

/// Crye JPC 2.0: front flap is a Crye AVS flap; back is a Crye #10 zipper.
fn crye_jpc_2() -> PlatformSpec {
    PlatformSpec {
        id: Uuid::new_v4(),
        sku: "crye-precision-jpc-2-0".into(),
        name: "Crye Precision JPC 2.0".into(),
        category: Some(PlatformCategory::PlateCarrier),
        plate_cuts: vec![PlateCut::SapiEsapi, PlateCut::Swimmer],
        sizes: vec![Size::Small, Size::Medium, Size::Large],
        slots: vec![
            MountSlot {
                id: "front-placard".into(),
                label: "Front placard / flap".into(),
                interface: InterfaceStandard::CryeAvsFlap,
                anchor: AnchorBox2D::new(30.0, 18.0, 40.0, 46.0, 10),
                pals_grid: None,
            },
            MountSlot {
                id: "back-panel".into(),
                label: "Back panel zipper".into(),
                interface: InterfaceStandard::CryeZip10,
                anchor: AnchorBox2D::new(30.0, 20.0, 40.0, 44.0, 10),
                pals_grid: None,
            },
        ],
    }
}

/// Ferro Concepts FCPC v5: front is G-Hook; back is a Ferro proprietary zipper.
fn ferro_fcpc_v5() -> PlatformSpec {
    PlatformSpec {
        id: Uuid::new_v4(),
        sku: "ferro-concepts-fcpc-v5".into(),
        name: "Ferro Concepts FCPC v5".into(),
        category: Some(PlatformCategory::PlateCarrier),
        plate_cuts: vec![PlateCut::SapiEsapi, PlateCut::Swimmer],
        sizes: vec![Size::Medium, Size::Large],
        slots: vec![
            MountSlot {
                id: "front-placard".into(),
                label: "Front placard / flap".into(),
                interface: InterfaceStandard::GhookLowProfile,
                anchor: AnchorBox2D::new(30.0, 18.0, 40.0, 46.0, 10),
                pals_grid: None,
            },
            MountSlot {
                id: "back-panel".into(),
                label: "Back panel zipper".into(),
                interface: InterfaceStandard::FerroZip,
                anchor: AnchorBox2D::new(30.0, 20.0, 40.0, 44.0, 10),
                pals_grid: None,
            },
        ],
    }
}

/// A 6-column PALS belt (e.g. a gun belt with a webbing field).
fn belt_platform() -> PlatformSpec {
    PlatformSpec {
        id: Uuid::new_v4(),
        sku: "pals-belt".into(),
        name: "PALS Belt".into(),
        category: Some(PlatformCategory::Belt),
        plate_cuts: vec![],
        sizes: vec![Size::Medium, Size::Large],
        slots: vec![MountSlot {
            id: "pals-grid".into(),
            label: "PALS webbing field".into(),
            interface: InterfaceStandard::StandardPals1In,
            anchor: AnchorBox2D::new(2.0, 40.0, 96.0, 20.0, 5),
            pals_grid: Some(PalsMatrix::new(6, 3)),
        }],
    }
}

/// Spiritus Systems Mk5 placard (1" SwiftClip / QASM).
fn spiritus_mk5_placard() -> ComponentSpec {
    ComponentSpec {
        id: Uuid::new_v4(),
        sku: "spiritus-mk5-placard".into(),
        name: "Spiritus Systems Mk5 Placard".into(),
        kind: ComponentKind::Placard,
        interface: InterfaceStandard::SwiftClip1InchQasm,
        anchor: AnchorBox2D::new(0.0, 0.0, 100.0, 100.0, 1),
        pals_footprint: None,
        child_slots: Vec::new(),
    }
}

/// A placard with a native Crye AVS flap mount.
fn avs_flap_placard() -> ComponentSpec {
    ComponentSpec {
        id: Uuid::new_v4(),
        sku: "crye-avs-detachable-flap".into(),
        name: "Crye AVS Detachable Flap".into(),
        kind: ComponentKind::Placard,
        interface: InterfaceStandard::CryeAvsFlap,
        anchor: AnchorBox2D::new(0.0, 0.0, 100.0, 100.0, 1),
        pals_footprint: None,
        child_slots: Vec::new(),
    }
}

/// Spiritus Systems zip-on back panel (Spiritus proprietary zipper).
fn spiritus_back_panel() -> ComponentSpec {
    ComponentSpec {
        id: Uuid::new_v4(),
        sku: "spiritus-zip-on-back-panel".into(),
        name: "Spiritus Systems Zip-On Back Panel".into(),
        kind: ComponentKind::BackPanel,
        interface: InterfaceStandard::SpiritusZip,
        anchor: AnchorBox2D::new(0.0, 0.0, 100.0, 100.0, 1),
        pals_footprint: None,
        child_slots: Vec::new(),
    }
}

/// A pouch with a 4x1 PALS footprint.
fn four_column_pouch() -> ComponentSpec {
    ComponentSpec {
        id: Uuid::new_v4(),
        sku: "four-column-utility-pouch".into(),
        name: "4-Column Utility Pouch".into(),
        kind: ComponentKind::BeltItem,
        interface: InterfaceStandard::StandardPals1In,
        anchor: AnchorBox2D::new(0.0, 0.0, 100.0, 100.0, 2),
        pals_footprint: Some(core_engine::graph::PalsFootprint { cols: 4, rows: 1 }),
        child_slots: Vec::new(),
    }
}

/// AXL AVP adapter: bolts onto a Crye AVS/JPC flap and exposes 1" SwiftClip.
fn axl_avp_adapter() -> AdapterBridge {
    AdapterBridge {
        id: Uuid::new_v4(),
        sku: "axl-avp-jpc".into(),
        name: "AXL AVP (AVS Vertical Placard) Adapter".into(),
        platform_interface: InterfaceStandard::CryeAvsFlap,
        component_interface: InterfaceStandard::SwiftClip1InchQasm,
        anchor: AnchorBox2D::new(0.0, 1.0, 100.0, 99.0, 1),
    }
}

/// Adapter chaining Crye AVS flaps to G-Hooks (for the single-hop test).
fn avs_to_ghook_adapter() -> AdapterBridge {
    AdapterBridge {
        id: Uuid::new_v4(),
        sku: "avs-to-ghook-adapter".into(),
        name: "AVS to G-Hook Adapter".into(),
        platform_interface: InterfaceStandard::CryeAvsFlap,
        component_interface: InterfaceStandard::GhookLowProfile,
        anchor: AnchorBox2D::new(0.0, 1.0, 100.0, 99.0, 1),
    }
}

/// Adapter chaining G-Hooks to Shaw ARC quad mounts (for the single-hop test).
fn ghook_to_arc_adapter() -> AdapterBridge {
    AdapterBridge {
        id: Uuid::new_v4(),
        sku: "ghook-to-arc-adapter".into(),
        name: "G-Hook to Shaw ARC Adapter".into(),
        platform_interface: InterfaceStandard::GhookLowProfile,
        component_interface: InterfaceStandard::ShawArcQuad,
        anchor: AnchorBox2D::new(0.0, 1.0, 100.0, 99.0, 1),
    }
}

// --- Test 1: single-hop adapter bridging --------------------------------------

#[test]
fn test_1_jpc_2_mk5_placard_requires_axl_avp_adapter() {
    let platform = crye_jpc_2();
    let placard = spiritus_mk5_placard();
    let adapter = axl_avp_adapter();
    let registry = vec![adapter.clone()];

    let result = resolve_mount(&platform, "front-placard", &placard, &registry);

    match result {
        CompatibilityResult::RequiresAdapter {
            required_adapters,
            anchor,
        } => {
            // Exactly one physical intermediary: the AXL AVP.
            assert_eq!(required_adapters, vec![adapter]);
            // Slot anchor -> adapter hardware -> placard, composited.
            assert_eq!(anchor.z_index, 12); // 10 (slot) + 1 (adapter) + 1 (placard)
            assert!((anchor.x_pct - 30.0).abs() < 1e-4);
            assert!((anchor.width_pct - 40.0).abs() < 1e-4);
            assert!((anchor.height_pct - 45.54).abs() < 1e-3); // 46% * 99% * 100%
        }
        other => panic!("expected RequiresAdapter, got {other:?}"),
    }
}

// --- Test 2: zipper gauge mismatch ---------------------------------------------

#[test]
fn test_2_fcpc_v5_rejects_spiritus_back_panel_zipper_gauge_mismatch() {
    let platform = ferro_fcpc_v5();
    let panel = spiritus_back_panel();

    // No adapter in the registry can bridge proprietary zipper tracks.
    let result = resolve_mount(&platform, "back-panel", &panel, &[]);

    assert_eq!(
        result,
        CompatibilityResult::Incompatible {
            reason: IncompatibilityReason::ZipperGaugeMismatch {
                slot_interface: InterfaceStandard::FerroZip,
                component_interface: InterfaceStandard::SpiritusZip,
            },
        }
    );
}

// --- Test 3: PALS grid boundary overflow ---------------------------------------

#[test]
fn test_3_pals_grid_rejects_four_column_pouch_at_index_four() {
    // Direct grid math: 4 + 4 = 8 columns required, 6 available.
    let mut grid = PalsMatrix::new(6, 3);
    assert!(!grid.can_fit(4, 0, 4, 1));
    assert_eq!(
        grid.mount(4, 0, 4, 1),
        Err(SpatialError::OutOfBounds {
            cols: 6,
            rows: 3,
            col_idx: 4,
            row_idx: 0,
            item_cols: 4,
            item_rows: 1,
        })
    );

    // The same overflow is rejected through the platform-level placement API.
    let mut belt = belt_platform();
    let pouch = four_column_pouch();
    let err = {
        let slot = belt.slot_mut("pals-grid").expect("belt exposes pals-grid");
        mount_on_pals(slot, &pouch, 4, 0).unwrap_err()
    };
    assert!(matches!(err, SpatialError::OutOfBounds { .. }));

    // Boundary sanity: a 4-column pouch fits exactly at index 2 (2 + 4 == 6),
    // and yields a correctly scaled sprite anchor.
    let slot = belt.slot_mut("pals-grid").unwrap();
    let anchor = mount_on_pals(slot, &pouch, 2, 0).expect("fits at index 2");
    assert!((anchor.x_pct - 34.0).abs() < 1e-4); // 2.0 + 2 cells * 16%
    assert!((anchor.width_pct - 64.0).abs() < 1e-4); // 96% * 4/6
    assert_eq!(anchor.z_index, 7); // 5 (slot) + 2 (pouch)
}

// --- Supporting behavior --------------------------------------------------------

#[test]
fn identical_interfaces_resolve_to_native_fit() {
    let platform = crye_jpc_2();
    let placard = avs_flap_placard();

    let result = resolve_mount(&platform, "front-placard", &placard, &[]);

    match result {
        CompatibilityResult::NativeFit { anchor } => {
            // Full-slot child anchor composed onto the slot's box.
            assert_eq!(anchor, AnchorBox2D::new(30.0, 18.0, 40.0, 46.0, 11));
        }
        other => panic!("expected NativeFit, got {other:?}"),
    }
}

#[test]
fn adapter_bridging_is_limited_to_a_single_hop() {
    // AVS flap -> G-Hook and G-Hook -> Shaw ARC adapters exist, but chaining
    // them (two physical intermediaries) must not satisfy an AVS -> ARC mount.
    let platform = crye_jpc_2();
    let arc_placard = ComponentSpec {
        id: Uuid::new_v4(),
        sku: "shaw-arc-quad-placard".into(),
        name: "Shaw ARC Quad Placard".into(),
        kind: ComponentKind::Placard,
        interface: InterfaceStandard::ShawArcQuad,
        anchor: AnchorBox2D::new(0.0, 0.0, 100.0, 100.0, 1),
        pals_footprint: None,
        child_slots: Vec::new(),
    };
    let registry = vec![avs_to_ghook_adapter(), ghook_to_arc_adapter()];

    let result = resolve_mount(&platform, "front-placard", &arc_placard, &registry);

    assert_eq!(
        result,
        CompatibilityResult::Incompatible {
            reason: IncompatibilityReason::NoAdapterFound {
                slot_interface: InterfaceStandard::CryeAvsFlap,
                component_interface: InterfaceStandard::ShawArcQuad,
            },
        }
    );
}

#[test]
fn cross_family_mounts_are_incompatible() {
    // A SwiftClip placard cannot mount onto a belt PALS grid, adapter or not.
    let belt = belt_platform();
    let placard = spiritus_mk5_placard();

    let result = resolve_mount(&belt, "pals-grid", &placard, &[axl_avp_adapter()]);

    assert_eq!(
        result,
        CompatibilityResult::Incompatible {
            reason: IncompatibilityReason::InterfaceFamilyMismatch {
                slot_interface: InterfaceStandard::StandardPals1In,
                component_interface: InterfaceStandard::SwiftClip1InchQasm,
            },
        }
    );
}

#[test]
fn unknown_slot_id_is_reported() {
    let platform = crye_jpc_2();

    let result = resolve_mount(&platform, "shoulder-loop", &spiritus_mk5_placard(), &[]);

    assert_eq!(
        result,
        CompatibilityResult::Incompatible {
            reason: IncompatibilityReason::UnknownSlot {
                slot_id: "shoulder-loop".into(),
            },
        }
    );
}

#[test]
fn occupied_pals_cells_block_second_mount() {
    let mut grid = PalsMatrix::new(6, 3);
    grid.mount(0, 0, 3, 2).expect("first mount succeeds");

    // Overlaps the first footprint.
    assert!(!grid.can_fit(2, 0, 3, 2));
    assert_eq!(
        grid.mount(2, 0, 3, 2),
        Err(SpatialError::AlreadyOccupied {
            col_idx: 2,
            row_idx: 0,
            item_cols: 3,
            item_rows: 2,
        })
    );

    // Zero-dimension footprints are rejected outright.
    assert_eq!(
        grid.mount(0, 0, 0, 1),
        Err(SpatialError::InvalidFootprint { item_cols: 0, item_rows: 1 })
    );

    // Disjoint placement still works.
    grid.mount(4, 2, 2, 1).expect("disjoint mount succeeds");
    assert_eq!(
        grid.occupied,
        vec![
            vec![true, true, true, false, false, false],
            vec![true, true, true, false, false, false],
            vec![false, false, false, false, true, true],
        ]
    );
}

#[test]
fn anchor_composition_is_nested_relative() {
    let parent = AnchorBox2D::new(20.0, 10.0, 50.0, 40.0, 5);
    let child = AnchorBox2D::new(50.0, 25.0, 20.0, 50.0, 3);

    let composed = parent.compose(&child);

    // x = 20 + 50% of 50, y = 10 + 25% of 40, w = 20% of 50, h = 50% of 40.
    assert!((composed.x_pct - 45.0).abs() < 1e-4);
    assert!((composed.y_pct - 20.0).abs() < 1e-4);
    assert!((composed.width_pct - 10.0).abs() < 1e-4);
    assert!((composed.height_pct - 20.0).abs() < 1e-4);
    assert_eq!(composed.z_index, 8);
}

#[test]
fn specs_and_results_round_trip_through_json() {
    let platform = crye_jpc_2();
    let json = to_json(&platform).expect("platform serializes");
    let parsed: PlatformSpec = from_json(&json).expect("platform deserializes");
    assert_eq!(parsed, platform);

    let result = resolve_mount(
        &platform,
        "front-placard",
        &spiritus_mk5_placard(),
        &[axl_avp_adapter()],
    );
    let json = to_json(&result).expect("result serializes");
    let parsed: CompatibilityResult = from_json(&json).expect("result deserializes");
    assert_eq!(parsed, result);
}
