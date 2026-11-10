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
use serde::Serialize;
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
/// items (including their child slots with current grid occupancy) and the
/// platform with its live PALS occupancy.
#[derive(Serialize)]
struct DraftStateDto {
    summary: LoadoutSummary,
    platform: core_engine::graph::PlatformSpec,
    mounted: Vec<MountedItem>,
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
        })
    }
}

// --- draft ------------------------------------------------------------------------

/// A stateful loadout under construction, owned by the browser. Call
/// [`Draft::state`] after every mutation to re-render.
#[wasm_bindgen]
pub struct Draft {
    catalog: Catalog,
    draft: LoadoutDraft,
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

    /// The full renderable draft state: `LoadoutSummary` (ordered layers,
    /// BOM, warnings), the platform with live PALS occupancy, and the live
    /// mounted items with their child slots.
    #[wasm_bindgen]
    pub fn state(&self) -> String {
        let platform = self.draft.platform().clone();
        to_json(&DraftStateDto {
            summary: self.draft.summary(),
            platform,
            mounted: self.draft.mounted().to_vec(),
        })
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

    #[test]
    fn embedded_catalog_parses_and_is_complete() {
        let catalog = parse_embedded_catalog();
        assert_eq!(catalog.platforms.len(), 29);
        assert_eq!(catalog.components.len(), 10);
        assert_eq!(catalog.adapter_bridges.len(), 4);
        assert!(catalog.platform("crye-precision-jpc-2-0").is_some());
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
}
