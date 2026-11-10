//! `wasm-engine` — WebAssembly bindings for the tactical gear `core-engine`.
//!
//! This crate is a thin, side-effect-free bridge: every compatibility decision,
//! PALS occupancy mutation, adapter auto-injection, and 2D anchor computation
//! happens inside `core-engine` — the exact same code the Axum REST API runs
//! server-side. The catalog seed is embedded at compile time
//! ([`CATALOG_SEED_JSON`]), so the demo boots with a complete, deterministic
//! catalog and performs zero network requests.
//!
//! The JavaScript surface mirrors the REST API shapes (platform summaries,
//! compatibility reports, `LoadoutSummary`) so the frontend code stays close
//! to the documented API contract.

#![forbid(unsafe_code)]
#![warn(missing_docs)]

use core_engine::catalog::Catalog;
use core_engine::graph::{
    resolve_slot_mount, CompatibilityResult, ComponentSpec, IncompatibilityReason, MountSlot,
};
use core_engine::loadout::{LoadoutDraft, LoadoutSummary, MountedItem};
use core_engine::pricing::Colorway;
use core_engine::regulation::{
    ComplianceReport, OperationalRole, RegulationPolicy, apply_preset, evaluate_compliance,
};
use serde::{Deserialize, Serialize};
use wasm_bindgen::prelude::*;

/// The deterministic catalog seed, embedded at compile time. Regenerate with
/// `cargo run -p data-ingest -- --generate-sample-csvs --input-dir <dir>
/// --output-dir seeds`; the build is byte-stable, so the embedded copy never
/// drifts from the pipeline that produced it.
const CATALOG_SEED_JSON: &str = include_str!("../../../seeds/catalog_seed.json");

// --- DTOs (mirroring the REST API shapes) ---------------------------------------

/// List-view summary of a platform.
#[derive(Serialize)]
struct PlatformSummaryDto {
    sku: String,
    name: String,
    category: Option<core_engine::graph::PlatformCategory>,
    slot_count: usize,
}

/// List-view summary of a role preset.
#[derive(Serialize, Deserialize)]
struct PresetSummaryDto {
    preset_id: String,
    name: String,
    role: Option<OperationalRole>,
    policy: RegulationPolicy,
    platform_sku: String,
    default_colorway: Colorway,
    placement_count: usize,
}

/// Summary of an adapter bridge a mount requires.
#[derive(Serialize)]
struct AdapterSummaryDto {
    sku: String,
    name: String,
}

/// One classified component in a compatibility report.
#[derive(Serialize)]
struct ComponentCompatEntryDto {
    sku: String,
    name: String,
    kind: core_engine::graph::ComponentKind,
    /// PALS footprint, when the component declares one (auto-placement).
    #[serde(skip_serializing_if = "Option::is_none")]
    pals_footprint: Option<core_engine::graph::PalsFootprint>,
    #[serde(skip_serializing_if = "Option::is_none")]
    adapters: Option<Vec<AdapterSummaryDto>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    reason: Option<IncompatibilityReason>,
}

/// Full classification of every catalog component against one slot.
#[derive(Serialize)]
struct CompatReportDto {
    native_fit: Vec<ComponentCompatEntryDto>,
    requires_adapter: Vec<ComponentCompatEntryDto>,
    incompatible: Vec<ComponentCompatEntryDto>,
}

/// Result envelope for state-mutating draft operations.
#[derive(Serialize)]
struct OpResultDto {
    ok: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<String>,
}

/// Full renderable draft state: the evaluation summary plus the live mounted
/// items (including their child slots with current grid occupancy), the
/// platform with its live PALS occupancy, and — when a regulation policy or
/// operational role is active — the compliance report.
#[derive(Serialize)]
struct DraftStateDto {
    summary: LoadoutSummary,
    platform: core_engine::graph::PlatformSpec,
    mounted: Vec<MountedItem>,
    #[serde(skip_serializing_if = "Option::is_none")]
    compliance: Option<ComplianceReport>,
    /// The colorway the compliance report was evaluated under, when set.
    #[serde(skip_serializing_if = "Option::is_none")]
    colorway: Option<Colorway>,
}

// --- engine ----------------------------------------------------------------------

/// The immutable product catalog plus the read-only queries the demo needs.
#[wasm_bindgen]
pub struct Engine {
    catalog: Catalog,
}

#[wasm_bindgen]
impl Engine {
    /// Load the engine with the compile-time embedded catalog seed.
    #[wasm_bindgen]
    pub fn embedded() -> Engine {
        Engine {
            catalog: parse_embedded_catalog(),
        }
    }

    /// Total number of catalog records (platforms + components + adapters).
    #[wasm_bindgen]
    pub fn catalog_len(&self) -> usize {
        self.catalog.len()
    }

    /// Platform list summaries (sku, name, category, slot count), grouped
    /// client-side.
    #[wasm_bindgen]
    pub fn platforms(&self) -> String {
        let platforms: Vec<PlatformSummaryDto> = self
            .catalog
            .platforms
            .iter()
            .map(|platform| PlatformSummaryDto {
                sku: platform.sku.clone(),
                name: platform.name.clone(),
                category: platform.category,
                slot_count: platform.slots.len(),
            })
            .collect();
        to_json(&platforms)
    }

    /// Full platform spec (slots, interfaces, anchors, PALS grids) by SKU, or
    /// `undefined` when the SKU is unknown.
    #[wasm_bindgen]
    pub fn platform(&self, sku: &str) -> Option<String> {
        self.catalog.platform(sku).map(to_json)
    }

    /// Classify every catalog component against one mount slot, passed as its
    /// serialized [`MountSlot`] JSON (works identically for platform slots
    /// and child slots exposed by mounted components). Returns
    /// `native_fit` / `requires_adapter` / `incompatible` arrays.
    #[wasm_bindgen]
    pub fn compatible_on_slot(&self, slot_json: &str) -> Option<String> {
        let slot: MountSlot = match serde_json::from_str(slot_json) {
            Ok(slot) => slot,
            Err(_) => return None,
        };
        Some(to_json(&classify_components_for_slot(&self.catalog, &slot)))
    }

    /// Start a new loadout draft on `platform_sku`. Fails when the platform
    /// is unknown.
    #[wasm_bindgen]
    pub fn create_draft(&self, platform_sku: &str) -> Result<Draft, JsValue> {
        let platform = self
            .catalog
            .platform(platform_sku)
            .cloned()
            .ok_or_else(|| JsValue::from_str(&format!("unknown platform sku `{platform_sku}`")))?;
        Ok(Draft {
            catalog: self.catalog.clone(),
            draft: LoadoutDraft::new(platform),
            colorway: None,
            policy: None,
            role: None,
        })
    }

    /// Role preset summaries (preset id, name, role, policy, platform,
    /// default colorway, placement count), in catalog order.
    #[wasm_bindgen]
    pub fn presets(&self) -> String {
        to_json(&preset_summaries(&self.catalog))
    }

    /// The full preset definition (placements, platform, policy, role,
    /// default colorway) by preset id, or `undefined` when unknown.
    #[wasm_bindgen]
    pub fn preset(&self, preset_id: &str) -> Option<String> {
        self.catalog.preset(preset_id).map(to_json)
    }

    /// Instantiate a role preset into a live draft: placements are applied
    /// through the normal engine validation path (adapter auto-injection,
    /// PALS collisions), and the draft's compliance context is seeded from
    /// the preset's declared colorway, policy, and role. Fails loudly for
    /// unknown preset ids and for placements the engine rejects.
    #[wasm_bindgen]
    pub fn load_preset(&self, preset_id: &str) -> Result<Draft, JsValue> {
        load_preset_into_draft(&self.catalog, preset_id).map_err(|error| JsValue::from_str(&error))
    }
}

// --- draft ------------------------------------------------------------------------

/// A stateful loadout under construction, owned by the browser. Call
/// [`Draft::state`] after every mutation to re-render.
#[wasm_bindgen]
pub struct Draft {
    catalog: Catalog,
    draft: LoadoutDraft,
    /// Chosen colorway, evaluated against the active policy.
    colorway: Option<Colorway>,
    /// Active regulation policy, when one is selected.
    policy: Option<RegulationPolicy>,
    /// Active operational role, when one is selected.
    role: Option<OperationalRole>,
}

#[wasm_bindgen]
impl Draft {
    /// Equip a component onto a slot (platform slot id or
    /// `<mount_id>.<child_slot_id>` path). Grid slots place at
    /// `(col, row)`; anchor-mounted slots ignore the indices.
    ///
    /// Returns `{"ok":true}` or `{"ok":false,"error":"..."}`; the caller
    /// refreshes via [`Draft::state`].
    #[wasm_bindgen]
    pub fn equip(&mut self, slot_id: &str, component_sku: &str, col: u8, row: u8) -> String {
        let result = self
            .draft
            .equip_component(&self.catalog, slot_id, component_sku, Some((col, row)));
        match result {
            Ok(_) => to_json(&OpResultDto {
                ok: true,
                error: None,
            }),
            Err(error) => to_json(&OpResultDto {
                ok: false,
                error: Some(error.to_string()),
            }),
        }
    }

    /// Unequip a mounted component (or adapter — adapters are refused with a
    /// descriptive error) by mount id.
    #[wasm_bindgen]
    pub fn unequip(&mut self, mount_id: &str) -> String {
        let result = self.draft.unequip_slot_item(mount_id);
        match result {
            Ok(_) => to_json(&OpResultDto {
                ok: true,
                error: None,
            }),
            Err(error) => to_json(&OpResultDto {
                ok: false,
                error: Some(error.to_string()),
            }),
        }
    }

    /// Set the compliance evaluation context: the chosen colorway plus the
    /// optional regulation policy and operational role. Values are the same
    /// snake_case strings the REST API accepts (`"multicam"`,
    /// `"us_army_ar670_1"`, `"sniper_dm"`); `undefined`/`null` clears a
    /// field. Unknown values fail loudly instead of being ignored.
    ///
    /// Returns `{"ok":true}` or `{"ok":false,"error":"..."}`; the caller
    /// refreshes via [`Draft::state`].
    #[wasm_bindgen]
    pub fn set_compliance_context(
        &mut self,
        colorway: Option<String>,
        policy: Option<String>,
        role: Option<String>,
    ) -> String {
        let colorway = match colorway.map(|raw| parse_variant::<Colorway>(&raw)) {
            None => None,
            Some(Err(error)) => return error,
            Some(Ok(value)) => Some(value),
        };
        let policy = match policy.map(|raw| parse_variant::<RegulationPolicy>(&raw)) {
            None => None,
            Some(Err(error)) => return error,
            Some(Ok(value)) => Some(value),
        };
        let role = match role.map(|raw| parse_variant::<OperationalRole>(&raw)) {
            None => None,
            Some(Err(error)) => return error,
            Some(Ok(value)) => Some(value),
        };
        self.colorway = colorway;
        self.policy = policy;
        self.role = role;
        to_json(&OpResultDto {
            ok: true,
            error: None,
        })
    }

    /// The full renderable draft state: `LoadoutSummary` (ordered layers,
    /// priced BOM, warnings), the platform with live PALS occupancy, the
    /// live mounted items with their child slots, and — when a policy or
    /// role is active — the compliance report.
    #[wasm_bindgen]
    pub fn state(&self) -> String {
        to_json(&draft_state(
            &self.catalog,
            &self.draft,
            self.colorway,
            self.policy,
            self.role,
        ))
    }
}

// --- plain helpers (unit-tested natively) ----------------------------------------

/// Parse the embedded catalog seed. Panics only on a corrupt seed file,
/// which is a build-time invariant, not a runtime condition.
fn parse_embedded_catalog() -> Catalog {
    serde_json::from_str(CATALOG_SEED_JSON).expect("embedded catalog seed is valid JSON")
}

/// Serialize a DTO compactly for the JS boundary.
fn to_json<T: Serialize>(value: &T) -> String {
    serde_json::to_string(value).expect("DTO serialization is infallible")
}

/// Build the preset list-view summaries, in catalog order.
fn preset_summaries(catalog: &Catalog) -> Vec<PresetSummaryDto> {
    catalog
        .role_presets
        .iter()
        .map(|preset| PresetSummaryDto {
            preset_id: preset.preset_id.clone(),
            name: preset.name.clone(),
            role: preset.role,
            policy: preset.policy,
            platform_sku: preset.platform_sku.clone(),
            default_colorway: preset.default_colorway,
            placement_count: preset.placements.len(),
        })
        .collect()
}

/// Instantiate a preset into a live draft with its compliance context
/// seeded from the preset's own declaration. Errors are human-readable
/// strings so both the wasm boundary and the native tests can surface them.
fn load_preset_into_draft(catalog: &Catalog, preset_id: &str) -> Result<Draft, String> {
    let preset = catalog
        .preset(preset_id)
        .ok_or_else(|| format!("unknown preset id `{preset_id}`"))?;
    let draft = apply_preset(catalog, preset).map_err(|error| error.to_string())?;
    Ok(Draft {
        catalog: catalog.clone(),
        draft,
        colorway: Some(preset.default_colorway),
        policy: Some(preset.policy),
        role: preset.role,
    })
}

/// Snapshot the renderable draft state, attaching the compliance report
/// when a policy or role is active.
fn draft_state(
    catalog: &Catalog,
    draft: &LoadoutDraft,
    colorway: Option<Colorway>,
    policy: Option<RegulationPolicy>,
    role: Option<OperationalRole>,
) -> DraftStateDto {
    let platform = draft.platform().clone();
    let compliance = if policy.is_some() || role.is_some() {
        Some(evaluate_compliance(
            catalog,
            draft,
            colorway.unwrap_or(platform.default_colorway),
            policy,
            role,
        ))
    } else {
        None
    };
    DraftStateDto {
        summary: draft.summary(),
        platform,
        mounted: draft.mounted().to_vec(),
        compliance,
        colorway,
    }
}

/// Parse one snake_case enum value (`"multicam"`, `"us_army_ar670_1"`) from
/// the JS boundary, returning a ready-to-return error string on failure.
/// Unknown values are loud failures, never silent fallbacks (R2).
fn parse_variant<T: serde::de::DeserializeOwned>(raw: &str) -> Result<T, String> {
    let value = serde_json::Value::String(raw.replace('"', ""));
    let parsed: T = serde_json::from_value(value).map_err(|error| {
        to_json(&OpResultDto {
            ok: false,
            error: Some(format!("invalid value `{raw}`: {error}")),
        })
    })?;
    Ok(parsed)
}

/// Classify every catalog component against `slot`, mirroring the REST
/// API's `CompatibleComponents` report.
fn classify_components_for_slot(catalog: &Catalog, slot: &MountSlot) -> CompatReportDto {
    let mut report = CompatReportDto {
        native_fit: Vec::new(),
        requires_adapter: Vec::new(),
        incompatible: Vec::new(),
    };
    for component in &catalog.components {
        let entry = component_entry(component);
        match resolve_slot_mount(slot, component, &catalog.adapter_bridges) {
            CompatibilityResult::NativeFit { .. } => report.native_fit.push(entry),
            CompatibilityResult::RequiresAdapter { required_adapters, .. } => {
                report.requires_adapter.push(ComponentCompatEntryDto {
                    adapters: Some(
                        required_adapters
                            .iter()
                            .map(|adapter| AdapterSummaryDto {
                                sku: adapter.sku.clone(),
                                name: adapter.name.clone(),
                            })
                            .collect(),
                    ),
                    ..entry
                });
            }
            CompatibilityResult::Incompatible { reason } => {
                report.incompatible.push(ComponentCompatEntryDto {
                    reason: Some(reason),
                    ..entry
                });
            }
        }
    }
    report
}

/// Base component entry (classification-agnostic fields).
fn component_entry(component: &ComponentSpec) -> ComponentCompatEntryDto {
    ComponentCompatEntryDto {
        sku: component.sku.clone(),
        name: component.name.clone(),
        kind: component.kind,
        pals_footprint: component.pals_footprint,
        adapters: None,
        reason: None,
    }
}

// --- native tests -----------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use core_engine::regulation::RolePreset;

    #[test]
    fn embedded_catalog_parses_and_is_complete() {
        let catalog = parse_embedded_catalog();
        assert_eq!(catalog.platforms.len(), 29);
        assert_eq!(catalog.components.len(), 12);
        assert_eq!(catalog.adapter_bridges.len(), 4);
        assert_eq!(catalog.role_presets.len(), 6);
        assert!(catalog.platform("crye-precision-jpc-2-0").is_some());
        assert!(catalog.preset("jtac").is_some());
        // Every seeded platform and component is priced.
        assert!(catalog
            .platforms
            .iter()
            .all(|platform| platform.msrp_cents > 0));
        assert!(catalog
            .components
            .iter()
            .all(|component| component.msrp_cents > 0));
    }

    #[test]
    fn slot_classification_matches_the_engine_registry() {
        let catalog = parse_embedded_catalog();
        let jpc = catalog.platform("crye-precision-jpc-2-0").unwrap();
        let front = jpc.slot("front_placard").unwrap();

        let report = classify_components_for_slot(&catalog, front);
        // No starter component natively mates with the Crye AVS flap.
        assert!(report.native_fit.is_empty());
        // SwiftClip placards mount through the AXL AVP.
        let mk5 = report
            .requires_adapter
            .iter()
            .find(|entry| entry.sku == "spiritus-mk5-placard")
            .expect("MK5 requires an adapter");
        assert_eq!(
            mk5.adapters.as_ref().unwrap()[0].sku,
            "axl-avp-jpc"
        );
        // Every catalog component lands in exactly one bucket.
        let total = report.native_fit.len()
            + report.requires_adapter.len()
            + report.incompatible.len();
        assert_eq!(total, catalog.components.len());
    }

    #[test]
    fn child_slot_classification_uses_the_same_engine() {
        let catalog = parse_embedded_catalog();
        let mk5 = catalog.component("spiritus-mk5-placard").unwrap();
        let field = mk5
            .child_slots
            .iter()
            .find(|slot| slot.id == "placard_front_pals")
            .expect("MK5 exposes its front PALS field");

        let report = classify_components_for_slot(&catalog, field);
        let kywi = report
            .native_fit
            .iter()
            .find(|entry| entry.sku == "esstac-kywi-556-double")
            .expect("KYWI natively mounts on 1-inch PALS webbing");
        assert!(kywi.adapters.is_none());
        // SwiftClip placards do not mount on plain webbing.
        assert!(report
            .incompatible
            .iter()
            .any(|entry| entry.sku == "spiritus-mk5-placard"));
    }

    #[test]
    fn preset_summaries_list_all_six_presets() {
        let catalog = parse_embedded_catalog();
        let presets = preset_summaries(&catalog);
        let ids: Vec<&str> = presets.iter().map(|p| p.preset_id.as_str()).collect();
        assert_eq!(
            ids,
            vec!["breacher", "jtac", "drone-operator", "sniper-dm", "recce", "le-patrol"]
        );
        let le = presets.iter().find(|p| p.preset_id == "le-patrol").unwrap();
        assert!(le.role.is_none(), "LE patrol is policy-only");
    }

    #[test]
    fn load_preset_applies_and_carries_compliance_context() {
        let catalog = parse_embedded_catalog();

        // Unknown preset ids fail loudly.
        assert!(load_preset_into_draft(&catalog, "ghost").is_err());

        let draft = load_preset_into_draft(&catalog, "jtac").expect("jtac preset applies");
        let state = draft_state(
            &draft.catalog,
            &draft.draft,
            draft.colorway,
            draft.policy,
            draft.role,
        );

        assert!(state.summary.valid);
        assert!(state.summary.errors.is_empty());
        assert!(state
            .summary
            .bom
            .iter()
            .any(|line| line.sku == "axl-avp-jpc" && line.is_adapter));
        assert!(state.summary.total_msrp_cents > 0);
        assert!(state.summary.total_best_price_cents > 0);

        // The compliance context was seeded from the preset declaration.
        let compliance = state.compliance.expect("jtac declares policy+role");
        assert!(compliance.compliant, "violations: {:?}", compliance.violations);
        assert_eq!(state.colorway, Some(Colorway::Multicam));

        // The full preset definition echoes placements for client editing.
        let preset: RolePreset = serde_json::from_str(
            &catalog.preset("jtac").map(to_json).expect("jtac preset exists"),
        )
        .expect("preset JSON");
        assert_eq!(preset.platform_sku, "crye-precision-jpc-2-0");
        assert_eq!(preset.placements.len(), 5);
        assert_eq!(preset.placements[0].slot_id, "front_placard");
    }

    #[test]
    fn compliance_context_parses_and_rejects_bad_values() {
        let catalog = parse_embedded_catalog();
        let platform = catalog
            .platform("crye-precision-jpc-2-0")
            .unwrap()
            .clone();
        let draft = LoadoutDraft::new(platform);

        // Without a policy or role, state carries no compliance report.
        let state = draft_state(&catalog, &draft, None, None, None);
        assert!(state.compliance.is_none());

        // AR 670-1 on a bare platform in Black reports typed violations.
        let state = draft_state(
            &catalog,
            &draft,
            Some(Colorway::Black),
            Some(RegulationPolicy::UsArmyAr670_1),
            None,
        );
        let compliance = state.compliance.expect("policy active");
        assert!(!compliance.compliant);
        assert!(compliance.violations.iter().any(|issue| matches!(
            issue.code,
            core_engine::regulation::ComplianceRuleCode::DisallowedColorway { .. }
        )));

        // Unknown enum values fail loudly instead of silently no-op'ing.
        assert!(parse_variant::<Colorway>("neon-pink").is_err());
        assert!(parse_variant::<RegulationPolicy>("geneva-convention").is_err());
        assert!(parse_variant::<OperationalRole>("wizard").is_err());
        assert!(parse_variant::<Colorway>("multicam").is_ok());

        // Clearing the context removes the compliance report again.
        let state = draft_state(&catalog, &draft, None, None, None);
        assert!(state.compliance.is_none());
    }
}
