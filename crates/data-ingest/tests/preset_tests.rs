//! Spec-04 acceptance tests: role presets apply through the normal engine
//! validation path (adapter auto-injection, PALS collisions) and every
//! seeded preset evaluates both structurally valid and fully compliant
//! against its own declared policy and role.

use core_engine::pricing::Colorway;
use core_engine::regulation::{apply_preset, evaluate_compliance};
use data_ingest::normalizer::normalize_catalog;
use data_ingest::sample_csvs::write_sample_csvs;
use std::sync::atomic::{AtomicU32, Ordering};

/// Unique temp dir per test (no `tempfile` dependency available).
fn temp_dir(tag: &str) -> std::path::PathBuf {
    static COUNTER: AtomicU32 = AtomicU32::new(0);
    let n = COUNTER.fetch_add(1, Ordering::SeqCst);
    std::env::temp_dir().join(format!(
        "data-ingest-preset-test-{}-{tag}-{}",
        std::process::id(),
        n
    ))
}

/// Ingest the sample CSVs from a fresh temp dir.
fn catalog() -> data_ingest::Catalog {
    let dir = temp_dir("presets");
    write_sample_csvs(&dir).expect("sample CSVs write");
    normalize_catalog(&dir).expect("sample CSVs ingest with zero errors").catalog
}

#[test]
fn every_seeded_preset_applies_and_evaluates_compliant() {
    let catalog = catalog();
    assert_eq!(catalog.role_presets.len(), 6, "exactly six curated presets");

    for preset in &catalog.role_presets {
        // Apply through the normal equip path: unknown SKUs, interface
        // mismatches, and PALS collisions must all fail loudly here.
        let draft = apply_preset(&catalog, preset).unwrap_or_else(|err| {
            panic!("preset {} must apply cleanly: {err}", preset.preset_id)
        });

        // The summary reports valid with a fully populated, priced BOM.
        let summary = draft.summary();
        assert!(summary.valid, "{}: preset must evaluate valid", preset.preset_id);
        assert!(summary.errors.is_empty(), "{}: no placement errors", preset.preset_id);
        assert!(!summary.bom.is_empty(), "{}: preset must mount components", preset.preset_id);
        assert!(summary.total_msrp_cents > 0, "{}: priced platform expected", preset.preset_id);
        assert!(summary.total_best_price_cents > 0, "{}: priced build expected", preset.preset_id);

        // And it is compliant under its own declared colorway/policy/role.
        let report = evaluate_compliance(
            &catalog,
            &draft,
            preset.default_colorway,
            Some(preset.policy),
            preset.role,
        );
        assert!(
            report.compliant,
            "{}: preset must satisfy its own policy+role; violations: {:?}",
            preset.preset_id,
            report.violations
        );
        // Suggestions are only meaningful for gaps; a clean preset has none
        // in its violations (there are no violations at all).
        assert!(report.violations.is_empty(), "{}: no violations", preset.preset_id);
    }
}

#[test]
fn presets_apply_byte_identically_across_runs() {
    let catalog_a = catalog();
    let catalog_b = catalog();
    for preset in &catalog_a.role_presets {
        let draft_a = apply_preset(&catalog_a, preset).expect("preset applies");
        let twin = catalog_b
            .preset(&preset.preset_id)
            .expect("preset id exists in the second catalog");
        let draft_b = apply_preset(&catalog_b, twin).expect("preset applies");
        assert_eq!(
            draft_a.summary(),
            draft_b.summary(),
            "preset {} must be byte-deterministic",
            preset.preset_id
        );
    }
}

#[test]
fn preset_boms_include_adapter_costs() {
    let catalog = catalog();
    // The JTAC preset mounts the MK5 on the JPC's Crye AVS flap, which
    // requires the AXL AVP adapter: the adapter must appear as a BOM line
    // carrying its own pricing.
    let preset = catalog.preset("jtac").expect("jtac preset");
    let draft = apply_preset(&catalog, preset).expect("jtac applies");
    let summary = draft.summary();

    let adapter_line = summary
        .bom
        .iter()
        .find(|line| line.is_adapter)
        .unwrap_or_else(|| panic!("jtac BOM must include the auto-injected adapter"));
    assert_eq!(adapter_line.sku, "axl-avp-jpc");
    assert_eq!(adapter_line.quantity, 1);
    assert!(adapter_line.msrp_cents > 0, "adapter line must carry MSRP");
    assert!(adapter_line.best_offer.is_some(), "adapter line must carry a best offer");

    // The totals include the adapter: platform + adapter + components.
    let bom_sum: u32 = summary
        .bom
        .iter()
        .map(|line| line.msrp_cents * line.quantity)
        .sum();
    let jpc_msrp = catalog
        .platform("crye-precision-jpc-2-0")
        .expect("JPC in catalog")
        .msrp_cents;
    assert_eq!(summary.total_msrp_cents, jpc_msrp + bom_sum);
}

#[test]
fn le_patrol_preset_is_policy_only_but_still_compliant() {
    let catalog = catalog();
    let preset = catalog.preset("le-patrol").expect("le-patrol preset");
    assert!(preset.role.is_none(), "LE patrol is policy-only");
    assert_eq!(preset.default_colorway, Colorway::Black);

    let draft = apply_preset(&catalog, preset).expect("le-patrol applies");
    let report = evaluate_compliance(
        &catalog,
        &draft,
        preset.default_colorway,
        Some(preset.policy),
        preset.role,
    );
    assert!(report.compliant, "black belt build satisfies LE patrol policy");
    // The holster drop zone mount provides the required holster retention.
    assert!(summary_has_component(&draft, "safariland-ubl-mid-ride"));
}

/// Whether a draft has mounted the given SKU.
fn summary_has_component(draft: &core_engine::loadout::LoadoutDraft, sku: &str) -> bool {
    draft.summary().bom.iter().any(|line| line.sku == sku)
}
