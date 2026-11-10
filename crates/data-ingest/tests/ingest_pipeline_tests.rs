//! End-to-end ingest pipeline tests: sample CSVs → normalization → seed
//! generation → `core_engine::resolve_mount` verification.

use core_engine::graph::{mount_on_pals, resolve_mount, CompatibilityResult, IncompatibilityReason};
use core_engine::interfaces::InterfaceStandard;
use data_ingest::normalizer::{normalize_catalog, slug_id};
use data_ingest::sample_csvs::write_sample_csvs;
use data_ingest::sql_gen::generate_seed_sql;
use data_ingest::{Catalog, IngestError};
use std::sync::atomic::{AtomicU32, Ordering};

/// Unique temp dir per test (no `tempfile` dependency available).
fn temp_dir(tag: &str) -> std::path::PathBuf {
    static COUNTER: AtomicU32 = AtomicU32::new(0);
    let n = COUNTER.fetch_add(1, Ordering::SeqCst);
    std::env::temp_dir().join(format!(
        "data-ingest-test-{}-{tag}-{}",
        std::process::id(),
        n
    ))
}

/// Ingest the sample CSVs from a fresh temp dir.
fn ingest_samples(tag: &str) -> (data_ingest::IngestReport, std::path::PathBuf) {
    let dir = temp_dir(tag);
    write_sample_csvs(&dir).expect("sample CSVs write");
    let report = normalize_catalog(&dir).expect("sample CSVs ingest with zero errors");
    (report, dir)
}

fn platform<'a>(catalog: &'a Catalog, name: &str) -> &'a core_engine::graph::PlatformSpec {
    catalog
        .platforms
        .iter()
        .find(|p| p.name == name)
        .unwrap_or_else(|| panic!("platform {name:?} missing from catalog"))
}

fn component<'a>(catalog: &'a Catalog, name: &str) -> &'a core_engine::graph::ComponentSpec {
    catalog
        .components
        .iter()
        .find(|c| c.name == name)
        .unwrap_or_else(|| panic!("component {name:?} missing from catalog"))
}

// --- end-to-end: ingest + seed generation ---------------------------------------

#[test]
fn end_to_end_ingest_generates_seed_files_with_zero_errors() {
    let (report, _dir) = ingest_samples("e2e");
    let catalog = &report.catalog;

    // All five sheets were read (19 + 4 + 4 + 3 + 19 schema rows).
    assert_eq!(
        report
            .rows_ingested
            .iter()
            .map(|(file, n)| (file.as_str(), *n))
            .collect::<Vec<_>>(),
        vec![
            ("plate_carriers.csv", 18),
            ("belts.csv", 4),
            ("chest_rigs.csv", 4),
            ("lbe.csv", 3),
            ("compatibility_schema.csv", 19),
        ]
    );

    // 18 carriers + 4 belts + 4 rigs + 3 LBE platforms, 12 components
    // (Spec-04 added the PTT and admin pouches), 4 adapters, 6 role presets.
    assert_eq!(catalog.platforms.len(), 29);
    assert_eq!(catalog.components.len(), 12);
    assert_eq!(catalog.adapter_bridges.len(), 4);
    assert_eq!(catalog.role_presets.len(), 6);
    assert_eq!(catalog.role_presets[0].preset_id, "breacher");

    // Representative rows normalized to the expected slug SKUs and standards.
    let jpc = platform(catalog, "JPC 2.0");
    assert_eq!(jpc.id, data_ingest::normalizer::deterministic_uuid("crye-precision-jpc-2-0"));
    assert_eq!(
        jpc.slot("front_placard").unwrap().interface,
        InterfaceStandard::CryeAvsFlap
    );
    assert_eq!(
        jpc.slot("rear_panel").unwrap().interface,
        InterfaceStandard::CryeZip10
    );

    let fcpc = platform(catalog, "FCPC v5");
    assert_eq!(
        fcpc.slot("front_placard").unwrap().interface,
        InterfaceStandard::GhookLowProfile
    );
    assert_eq!(
        fcpc.slot("rear_panel").unwrap().interface,
        InterfaceStandard::FerroZip
    );
    assert_eq!(
        fcpc.slot("cummerbund_left").unwrap().interface,
        InterfaceStandard::ProprietaryTegris
    );

    let lv119 = platform(catalog, "LV-119 Overt");
    assert_eq!(
        lv119.slot("front_placard").unwrap().interface,
        InterfaceStandard::SwiftClip1InchQasm
    );
    assert_eq!(
        lv119.slot("rear_panel").unwrap().interface,
        InterfaceStandard::SpiritusZip
    );

    let smu = platform(catalog, "SMU Belt");
    assert_eq!(
        smu.slot("belt_outer_pals").unwrap().interface,
        InterfaceStandard::StandardPals1In
    );
    let belt_grid = &smu.slot("belt_outer_pals").unwrap().pals_grid;
    assert_eq!(
        belt_grid.as_ref().map(|g| (g.cols, g.rows)),
        Some((18, 2))
    );

    let mrb = platform(catalog, "MRB 2.0");
    assert_eq!(
        mrb.slot("rig_main_cell").unwrap().interface,
        InterfaceStandard::StandardPals1In
    );
    assert_eq!(
        mrb.slot("wing_left").unwrap().interface,
        InterfaceStandard::SwiftClip1InchQasm
    );

    // Determinism: re-running the pipeline yields a byte-identical catalog.
    let (report_again, _dir2) = ingest_samples("e2e-determinism");
    assert_eq!(report_again.catalog, *catalog);

    // Seed generation produces valid, idempotent SQL.
    let sql = generate_seed_sql(catalog);
    assert!(sql.contains("CREATE TABLE IF NOT EXISTS platforms"));
    assert!(sql.contains("CREATE TABLE IF NOT EXISTS mount_slots"));
    assert!(sql.contains("CREATE TABLE IF NOT EXISTS components"));
    assert!(sql.contains("CREATE TABLE IF NOT EXISTS adapter_bridges"));
    assert_eq!(sql.matches("ON CONFLICT (id) DO UPDATE").count(), 45); // 29 + 12 + 4
    assert_eq!(sql.matches("ON CONFLICT (preset_id) DO UPDATE").count(), 6);
    assert_eq!(
        sql.matches("ON CONFLICT (platform_id, slot_id) DO UPDATE").count(),
        catalog.platforms.iter().map(|p| p.slots.len()).sum::<usize>()
    );
}

#[test]
fn catalog_seed_json_round_trips() {
    let (report, _dir) = ingest_samples("json");
    let catalog = &report.catalog;

    let json = serde_json::to_string_pretty(catalog).expect("catalog serializes");
    let parsed: Catalog = serde_json::from_str(&json).expect("catalog parses back");
    assert_eq!(parsed, *catalog);

    // The JSON seed file itself is written by the CLI; verify the same path
    // the binary uses produces parseable output.
    let seed = serde_json::to_string(catalog).expect("compact seed serializes");
    assert!(seed.contains("JPC 2.0"));
    assert!(seed.contains("Spiritus Systems MK5 Placard"));
}

// --- end-to-end: resolve_mount across ingested records ---------------------------

#[test]
fn crye_jpc_2_mk5_placard_requires_axl_avp_adapter_from_seed_registry() {
    let (report, _dir) = ingest_samples("jpc");
    let catalog = &report.catalog;

    let jpc = platform(catalog, "JPC 2.0");
    let mk5 = component(catalog, "Spiritus Systems MK5 Placard");

    let result = resolve_mount(jpc, "front_placard", mk5, &catalog.adapter_bridges);
    match &result {
        CompatibilityResult::RequiresAdapter { required_adapters, .. } => {
            assert_eq!(required_adapters.len(), 1);
            assert_eq!(
                required_adapters[0].name,
                "AXL AVP (AVS Vertical Placard) Adapter"
            );
        }
        other => panic!("expected RequiresAdapter, got {other:?}"),
    }
    assert!(result.is_compatible());
}

#[test]
fn ferro_fcpc_v5_rejects_crye_zip_pack_with_gauge_mismatch() {
    let (report, _dir) = ingest_samples("fcpc");
    let catalog = &report.catalog;

    let fcpc = platform(catalog, "FCPC v5");
    let crye_pack = component(catalog, "Crye Zip-On Pack 2.0");

    let result = resolve_mount(fcpc, "rear_panel", crye_pack, &catalog.adapter_bridges);
    assert_eq!(
        result,
        CompatibilityResult::Incompatible {
            reason: IncompatibilityReason::ZipperGaugeMismatch {
                slot_interface: InterfaceStandard::FerroZip,
                component_interface: InterfaceStandard::CryeZip10,
            },
        }
    );
}

#[test]
fn spiritus_lv_119_mk5_placard_is_a_native_fit() {
    let (report, _dir) = ingest_samples("lv119");
    let catalog = &report.catalog;

    let lv119 = platform(catalog, "LV-119 Overt");
    let mk5 = component(catalog, "Spiritus Systems MK5 Placard");

    let result = resolve_mount(lv119, "front_placard", mk5, &catalog.adapter_bridges);
    assert!(matches!(result, CompatibilityResult::NativeFit { .. }));
}

#[test]
fn aws_smu_belt_accepts_ubl_holster_on_pals_grid() {
    let (report, _dir) = ingest_samples("belt");
    let catalog = &report.catalog;

    let smu = platform(catalog, "SMU Belt");
    let ubl = component(catalog, "Safariland UBL Mid-Ride");

    // Interface resolves natively (1" PALS both sides).
    let result = resolve_mount(smu, "belt_outer_pals", ubl, &catalog.adapter_bridges);
    assert!(matches!(result, CompatibilityResult::NativeFit { .. }));

    // Placement: the UBL's 2x3 footprint fills the 2x3 holster drop zone
    // exactly, and the 2x2 KYWI fits the 18x2 outer field.
    let mut smu = smu.clone();
    let holster = smu.slot_mut("holster_drop_zone").unwrap();
    let anchor = mount_on_pals(holster, ubl, 0, 0).expect("UBL fills the 2x3 drop zone");
    assert_eq!(anchor.z_index, 7); // holster slot z 6 + component z 1
    // The 3-row-tall UBL can never fit the 2-row outer PALS field.
    let outer = smu.slot_mut("belt_outer_pals").unwrap();
    let err = mount_on_pals(outer, ubl, 0, 0).unwrap_err();
    assert!(matches!(err, core_engine::SpatialError::OutOfBounds { .. }));
}

#[test]
fn crye_mrb_2_0_main_cell_accepts_pals_pouch() {
    let (report, _dir) = ingest_samples("mrb");
    let catalog = &report.catalog;

    let mrb = platform(catalog, "MRB 2.0");
    let kywi = component(catalog, "Esstac KYWI 5.56 Double");

    let result = resolve_mount(mrb, "rig_main_cell", kywi, &catalog.adapter_bridges);
    assert!(matches!(result, CompatibilityResult::NativeFit { .. }));

    let mut mrb = mrb.clone();
    let slot = mrb.slot_mut("rig_main_cell").unwrap();
    mount_on_pals(slot, kywi, 4, 1).expect("KYWI fits the 6x3 main cell");
    let slot = mrb.slot_mut("rig_main_cell").unwrap();
    let err = mount_on_pals(slot, kywi, 5, 1).unwrap_err();
    assert!(matches!(err, core_engine::SpatialError::OutOfBounds { .. }));
}

// --- zero silent fallbacks --------------------------------------------------------

#[test]
fn unmapped_interface_fails_loudly_with_row_column_and_value() {
    let dir = temp_dir("unmapped");
    write_sample_csvs(&dir).expect("sample CSVs write");

    // Corrupt row 4 (FCPC v5) front placard interface with an unknown string.
    let path = dir.join("plate_carriers.csv");
    let raw = std::fs::read_to_string(&path).expect("read sample csv");
    let mut lines: Vec<String> = raw.lines().map(str::to_owned).collect();
    let fields: Vec<String> = lines[4].split(',').map(str::to_owned).collect();
    let mut fixed = fields;
    fixed[3] = "\"Ferro Flux-Quantum Modular Weave\"".to_owned();
    lines[4] = fixed.join(",");

    {
        use std::io::Write;
        let mut file = std::fs::File::create(&path).expect("rewrite sample csv");
        for line in lines {
            writeln!(file, "{line}").expect("write line");
        }
    }

    let err = normalize_catalog(&dir).expect_err("must fail on unmappable interface");
    match err {
        IngestError::UnmappedInterface { row, column, raw_value } => {
            assert_eq!(row, 4);
            assert_eq!(column, "Front Placard Interface");
            assert_eq!(raw_value, "Ferro Flux-Quantum Modular Weave");
        }
        other => panic!("expected UnmappedInterface, got {other:?}"),
    }
}

#[test]
fn unmapped_plate_cut_fails_loudly() {
    let dir = temp_dir("platecut");
    write_sample_csvs(&dir).expect("sample CSVs write");

    let path = dir.join("plate_carriers.csv");
    let raw = std::fs::read_to_string(&path).expect("read sample csv");
    let mut lines: Vec<String> = raw.lines().map(str::to_owned).collect();
    let fields: Vec<String> = lines[2].split(',').map(str::to_owned).collect();
    let mut fixed = fields;
    fixed[2] = "\"Hexagonal Covert Cut\"".to_owned();
    lines[2] = fixed.join(",");

    {
        use std::io::Write;
        let mut file = std::fs::File::create(&path).expect("rewrite sample csv");
        for line in lines {
            writeln!(file, "{line}").expect("write line");
        }
    }

    let err = normalize_catalog(&dir).expect_err("must fail on unmappable plate cut");
    match err {
        IngestError::UnmappedPlateCut { row, raw_value, .. } => {
            assert_eq!(row, 2);
            assert_eq!(raw_value, "Hexagonal Covert Cut");
        }
        other => panic!("expected UnmappedPlateCut, got {other:?}"),
    }
}

#[test]
fn slug_ids_match_spec_examples() {
    assert_eq!(slug_id("Crye Precision", "JPC 2.0"), "crye-precision-jpc-2-0");
    assert_eq!(
        slug_id("Ferro Concepts", "Bison Belt"),
        "ferro-concepts-bison-belt"
    );
}
