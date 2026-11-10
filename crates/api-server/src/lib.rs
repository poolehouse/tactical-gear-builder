//! `api-server` — the Axum REST API for the tactical gear visual builder.
//!
//! Serves catalog queries, per-slot compatibility discovery, and full loadout
//! evaluation against the pure `core-engine` compute layer. The catalog is
//! loaded from `catalog_seed.json` when available and otherwise synthesized
//! through the `data-ingest` normalizer, so the API (and `cargo test`) always
//! boots with a complete catalog.
//!
//! Routes:
//! - `GET  /health`
//! - `GET  /api/v1/catalog/platforms` (`?category=`)
//! - `GET  /api/v1/catalog/platforms/{sku}`
//! - `GET  /api/v1/catalog/platforms/{sku}/compatible-components?slot_id=`
//! - `GET  /api/v1/catalog/presets`
//! - `POST /api/v1/loadouts/evaluate`
//! - `POST /api/v1/loadouts/from-preset/{preset_id}`

#![forbid(unsafe_code)]
#![warn(missing_docs)]

use std::collections::HashMap;
use std::path::Path;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Arc;

use axum::extract::{Path as PathParam, Query, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use core_engine::catalog::Catalog;
use core_engine::graph::{
    resolve_mount, CompatibilityResult, ComponentKind, ComponentSpec, IncompatibilityReason,
    PlatformCategory, PlatformSpec,
};
use core_engine::loadout::{LoadoutDraft, LoadoutSummary};
use core_engine::pricing::Colorway;
use core_engine::regulation::{
    ComplianceReport, OperationalRole, PresetPlacement, RegulationPolicy, evaluate_compliance,
};
use serde::{Deserialize, Serialize};
use thiserror::Error;
use tower_http::cors::CorsLayer;
use tower_http::trace::TraceLayer;

/// Shared application state: the immutable product catalog.
#[derive(Clone)]
pub struct AppState {
    catalog: Arc<Catalog>,
}

impl AppState {
    /// Wrap a catalog in shared state.
    pub fn new(catalog: Catalog) -> Self {
        Self {
            catalog: Arc::new(catalog),
        }
    }

    /// The shared catalog.
    pub fn catalog(&self) -> &Catalog {
        &self.catalog
    }
}

/// Build the full API router — all v1 routes, HTTP tracing, and permissive
/// CORS — over `state`.
pub fn build_router(state: AppState) -> Router {
    Router::new()
        .route("/health", get(health))
        .route("/api/v1/catalog/platforms", get(list_platforms))
        .route("/api/v1/catalog/platforms/{sku}", get(get_platform))
        .route(
            "/api/v1/catalog/platforms/{sku}/compatible-components",
            get(compatible_components),
        )
        .route("/api/v1/catalog/presets", get(list_presets))
        .route("/api/v1/loadouts/evaluate", post(evaluate_loadout))
        .route(
            "/api/v1/loadouts/from-preset/{preset_id}",
            post(loadout_from_preset),
        )
        .layer(TraceLayer::new_for_http())
        .layer(CorsLayer::permissive())
        .with_state(state)
}

// --- catalog bootstrap -----------------------------------------------------------

/// Load the product catalog.
///
/// Reads `catalog_seed.json` from `seed_path` when that file exists;
/// otherwise synthesizes the catalog through the `data-ingest` normalizer
/// (sample sheet CSVs → normalized catalog), so the API and `cargo test`
/// always boot with a full catalog even without a pre-generated seed file.
pub fn load_catalog(seed_path: Option<&Path>) -> Result<Catalog, ApiError> {
    if let Some(path) = seed_path {
        if path.is_file() {
            let raw = std::fs::read_to_string(path).map_err(|error| {
                ApiError::Catalog(format!("reading {}: {error}", path.display()))
            })?;
            return serde_json::from_str(&raw).map_err(|error| {
                ApiError::Catalog(format!("parsing {}: {error}", path.display()))
            });
        }
    }
    generate_catalog_fallback()
}

/// Synthesize the catalog in-memory via the data-ingest normalizer. Uses a
/// unique scratch directory per call (tests run in parallel threads).
fn generate_catalog_fallback() -> Result<Catalog, ApiError> {
    static SEED_DIR_SEQ: AtomicU32 = AtomicU32::new(0);
    let seq = SEED_DIR_SEQ.fetch_add(1, Ordering::SeqCst);
    let dir = std::env::temp_dir().join(format!("tgb-api-seed-{}-{seq}", std::process::id()));

    data_ingest::sample_csvs::write_sample_csvs(&dir)
        .map_err(|error| ApiError::Catalog(format!("sample CSV generation: {error}")))?;
    let report = data_ingest::normalizer::normalize_catalog(&dir)
        .map_err(|error| ApiError::Catalog(format!("catalog normalization: {error}")))?;

    // The normalized catalog is fully owned; drop the scratch CSVs.
    let _ = std::fs::remove_dir_all(&dir);
    Ok(report.catalog)
}

// --- errors -----------------------------------------------------------------------

/// API error responses, mapped onto HTTP status codes.
#[derive(Debug, Error)]
pub enum ApiError {
    /// No platform with the requested SKU exists (404).
    #[error("unknown platform sku `{0}`")]
    UnknownPlatform(String),
    /// No role preset with the requested id exists (404).
    #[error("unknown preset id `{0}`")]
    UnknownPreset(String),
    /// A required query parameter is missing (400).
    #[error("missing required query parameter `{0}`")]
    MissingParam(String),
    /// A parameter holds an unrecognized value (400).
    #[error("invalid `{param}` value: {value}")]
    InvalidParam {
        /// The offending parameter name.
        param: String,
        /// The offending raw value.
        value: String,
    },
    /// The request body could not be parsed (400).
    #[error("invalid request body: {0}")]
    InvalidBody(String),
    /// Catalog bootstrap failed (500).
    #[error("catalog bootstrap failed: {0}")]
    Catalog(String),
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let status = match &self {
            ApiError::UnknownPlatform(_) | ApiError::UnknownPreset(_) => StatusCode::NOT_FOUND,
            ApiError::MissingParam(_) | ApiError::InvalidParam { .. } => StatusCode::BAD_REQUEST,
            ApiError::InvalidBody(_) => StatusCode::BAD_REQUEST,
            ApiError::Catalog(_) => StatusCode::INTERNAL_SERVER_ERROR,
        };
        (status, Json(serde_json::json!({ "error": self.to_string() }))).into_response()
    }
}

// --- response DTOs -----------------------------------------------------------------

/// List-view summary of a platform.
#[derive(Debug, Clone, Serialize)]
pub struct PlatformSummary {
    /// SKU slug.
    pub sku: String,
    /// Product name.
    pub name: String,
    /// Platform category, when known.
    pub category: Option<PlatformCategory>,
    /// Number of mount slots the platform exposes.
    pub slot_count: usize,
}

/// Summary of an adapter bridge that a mount requires.
#[derive(Debug, Clone, Serialize)]
pub struct AdapterSummary {
    /// SKU slug.
    pub sku: String,
    /// Product name.
    pub name: String,
}

/// One classified component in a compatibility report.
#[derive(Debug, Clone, Serialize)]
pub struct ComponentCompatEntry {
    /// SKU slug.
    pub sku: String,
    /// Product name.
    pub name: String,
    /// Component category.
    pub kind: ComponentKind,
    /// Adapters that would bridge the mount (`requires_adapter` only).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub adapters: Option<Vec<AdapterSummary>>,
    /// Why the mount is impossible (`incompatible` only).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reason: Option<IncompatibilityReason>,
}

/// The full classification of every catalog component against one slot.
#[derive(Debug, Clone, Serialize)]
pub struct CompatReport {
    /// Components that mount directly.
    pub native_fit: Vec<ComponentCompatEntry>,
    /// Components that mount through exactly one adapter, with the adapter
    /// each requires.
    pub requires_adapter: Vec<ComponentCompatEntry>,
    /// Components that cannot mount at all, with the reason.
    pub incompatible: Vec<ComponentCompatEntry>,
}

/// `POST /api/v1/loadouts/evaluate` request body.
#[derive(Debug, Clone, Deserialize)]
pub struct EvaluateRequest {
    /// SKU of the base platform.
    pub platform_sku: String,
    /// Mounts to apply, in order.
    pub placements: Vec<PlacementRequest>,
    /// Chosen colorway; defaults to the platform's default colorway.
    #[serde(default)]
    pub colorway: Option<Colorway>,
    /// Regulation policy to validate against (`us_army_ar670_1`, ...).
    #[serde(default)]
    pub regulation_policy: Option<RegulationPolicy>,
    /// Operational role to validate against (`jtac`, `sniper_dm`, ...).
    #[serde(default)]
    pub operational_role: Option<OperationalRole>,
}

/// One placement in an [`EvaluateRequest`].
#[derive(Debug, Clone, Deserialize)]
pub struct PlacementRequest {
    /// Target slot id or `<mount_id>.<child_slot_id>` path.
    pub slot_id: String,
    /// SKU of the component to equip.
    pub component_sku: String,
    /// PALS column index (grid mounts).
    pub col_idx: u8,
    /// PALS row index (grid mounts).
    pub row_idx: u8,
}

/// `POST /api/v1/loadouts/evaluate` (and `from-preset`) response: the full
/// loadout summary with the compliance report flattened alongside it.
#[derive(Debug, Clone, Serialize)]
pub struct EvaluateResponse {
    /// The loadout evaluation summary (valid, layers, BOM, pricing, errors).
    #[serde(flatten)]
    pub summary: LoadoutSummary,
    /// Regulation/role compliance report. Present only when the request
    /// named a policy or role; violations are semantic results, never HTTP
    /// errors (R4).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub compliance: Option<ComplianceReport>,
}

/// `POST /api/v1/loadouts/from-preset/{preset_id}` response.
#[derive(Debug, Clone, Serialize)]
pub struct FromPresetResponse {
    /// The evaluated preset build and its compliance report.
    #[serde(flatten)]
    pub evaluation: EvaluateResponse,
    /// The preset id that was applied.
    pub preset_id: String,
    /// The platform the preset builds on.
    pub platform_sku: String,
    /// The placements that were applied, so a client can continue editing
    /// the preset build through `/evaluate` afterwards.
    pub placements: Vec<PresetPlacement>,
}

/// List-view summary of a role preset.
#[derive(Debug, Clone, Serialize)]
pub struct PresetSummary {
    /// Preset id slug (`"jtac"`).
    pub preset_id: String,
    /// Display name.
    pub name: String,
    /// Operational role the preset targets, when it has one.
    pub role: Option<OperationalRole>,
    /// Regulation policy the preset validates against.
    pub policy: RegulationPolicy,
    /// Platform the preset builds on.
    pub platform_sku: String,
    /// The preset's default colorway.
    pub default_colorway: Colorway,
    /// How many placements the preset applies.
    pub placement_count: usize,
}

// --- handlers ----------------------------------------------------------------------

/// `GET /health` — liveness probe.
async fn health() -> Json<serde_json::Value> {
    Json(serde_json::json!({ "status": "ok" }))
}

/// `GET /api/v1/catalog/platforms` — all platforms, optionally filtered by
/// `?category=plate_carrier|belt|chest_rig|lbe`.
async fn list_platforms(
    State(state): State<AppState>,
    Query(params): Query<HashMap<String, String>>,
) -> Result<Json<Vec<PlatformSummary>>, ApiError> {
    let category = match params.get("category") {
        None => None,
        Some(raw) => Some(parse_category(raw)?),
    };
    let platforms = state
        .catalog()
        .platforms
        .iter()
        .filter(|platform| match category {
            None => true,
            Some(category) => platform.category == Some(category),
        })
        .map(platform_summary)
        .collect();
    Ok(Json(platforms))
}

/// `GET /api/v1/catalog/platforms/{sku}` — platform details including its
/// mount slot definitions.
async fn get_platform(
    State(state): State<AppState>,
    PathParam(sku): PathParam<String>,
) -> Result<Json<PlatformSpec>, ApiError> {
    let platform = state
        .catalog()
        .platform(&sku)
        .ok_or_else(|| ApiError::UnknownPlatform(sku.clone()))?;
    Ok(Json(platform.clone()))
}

/// `GET /api/v1/catalog/platforms/{sku}/compatible-components?slot_id=` —
/// evaluates every catalog component against the platform's slot, classified
/// into `native_fit`, `requires_adapter`, and `incompatible`.
async fn compatible_components(
    State(state): State<AppState>,
    PathParam(sku): PathParam<String>,
    Query(params): Query<HashMap<String, String>>,
) -> Result<Json<CompatReport>, ApiError> {
    let catalog = state.catalog();
    let platform = catalog
        .platform(&sku)
        .ok_or_else(|| ApiError::UnknownPlatform(sku.clone()))?;
    let slot_id = params
        .get("slot_id")
        .ok_or_else(|| ApiError::MissingParam("slot_id".to_owned()))?;
    if platform.slot(slot_id).is_none() {
        return Err(ApiError::InvalidParam {
            param: "slot_id".to_owned(),
            value: slot_id.clone(),
        });
    }

    let mut report = CompatReport {
        native_fit: Vec::new(),
        requires_adapter: Vec::new(),
        incompatible: Vec::new(),
    };
    for component in &catalog.components {
        match resolve_mount(platform, slot_id, component, &catalog.adapter_bridges) {
            CompatibilityResult::NativeFit { .. } => {
                report.native_fit.push(component_entry(component, None, None));
            }
            CompatibilityResult::RequiresAdapter { required_adapters, .. } => {
                let adapters = required_adapters
                    .iter()
                    .map(|adapter| AdapterSummary {
                        sku: adapter.sku.clone(),
                        name: adapter.name.clone(),
                    })
                    .collect();
                report
                    .requires_adapter
                    .push(component_entry(component, Some(adapters), None));
            }
            CompatibilityResult::Incompatible { reason } => {
                report
                    .incompatible
                    .push(component_entry(component, None, Some(reason)));
            }
        }
    }
    Ok(Json(report))
}

/// `GET /api/v1/catalog/presets` — the curated role presets, in catalog
/// order.
async fn list_presets(State(state): State<AppState>) -> Json<Vec<PresetSummary>> {
    let presets = state
        .catalog()
        .role_presets
        .iter()
        .map(|preset| PresetSummary {
            preset_id: preset.preset_id.clone(),
            name: preset.name.clone(),
            role: preset.role,
            policy: preset.policy,
            platform_sku: preset.platform_sku.clone(),
            default_colorway: preset.default_colorway,
            placement_count: preset.placements.len(),
        })
        .collect();
    Json(presets)
}

/// `POST /api/v1/loadouts/evaluate` — runs a [`LoadoutDraft`] over the
/// placement list and returns the full evaluation summary plus, when a
/// policy or role was named, the compliance report. Individual failed
/// placements and compliance violations are reported in the body with HTTP
/// 200 (R4); an unknown platform is a 404.
async fn evaluate_loadout(
    State(state): State<AppState>,
    Json(payload): Json<EvaluateRequest>,
) -> Result<Json<EvaluateResponse>, ApiError> {
    let catalog = state.catalog();
    let platform = catalog
        .platform(&payload.platform_sku)
        .ok_or_else(|| ApiError::UnknownPlatform(payload.platform_sku.clone()))?
        .clone();

    let placements: Vec<(String, String, u8, u8)> = payload
        .placements
        .iter()
        .map(|p| (p.slot_id.clone(), p.component_sku.clone(), p.col_idx, p.row_idx))
        .collect();

    Ok(Json(evaluate_placements(
        catalog,
        &platform,
        &placements,
        payload.colorway.unwrap_or(platform.default_colorway),
        payload.regulation_policy,
        payload.operational_role,
    )))
}

/// `POST /api/v1/loadouts/from-preset/{preset_id}` — instantiates a curated
/// role preset through the normal engine validation path and evaluates it
/// against its own declared colorway, policy, and role. Unknown preset ids
/// are 404s; a preset placement that fails engine validation is reported in
/// the body with HTTP 200 (R4), exactly like `/evaluate`.
async fn loadout_from_preset(
    State(state): State<AppState>,
    PathParam(preset_id): PathParam<String>,
) -> Result<Json<FromPresetResponse>, ApiError> {
    let catalog = state.catalog();
    let preset = catalog
        .preset(&preset_id)
        .ok_or_else(|| ApiError::UnknownPreset(preset_id.clone()))?;
    let platform = catalog
        .platform(&preset.platform_sku)
        .cloned()
        .ok_or_else(|| ApiError::Catalog(format!(
            "preset `{preset_id}` references unknown platform `{}`",
            preset.platform_sku
        )))?;

    let placements: Vec<(String, String, u8, u8)> = preset
        .placements
        .iter()
        .map(|p| (p.slot_id.clone(), p.component_sku.clone(), p.col, p.row))
        .collect();
    let evaluation = evaluate_placements(
        catalog,
        &platform,
        &placements,
        preset.default_colorway,
        Some(preset.policy),
        preset.role,
    );

    Ok(Json(FromPresetResponse {
        evaluation,
        preset_id: preset.preset_id.clone(),
        platform_sku: preset.platform_sku.clone(),
        placements: preset.placements.clone(),
    }))
}

// --- helpers ----------------------------------------------------------------------

/// Shared evaluation core for `/evaluate` and `/from-preset`: applies the
/// placements in order (collecting per-placement errors without aborting),
/// snapshots the summary, and — when a policy or role is active — attaches
/// the compliance report.
fn evaluate_placements(
    catalog: &Catalog,
    platform: &PlatformSpec,
    placements: &[(String, String, u8, u8)],
    colorway: Colorway,
    policy: Option<RegulationPolicy>,
    role: Option<OperationalRole>,
) -> EvaluateResponse {
    let mut draft = LoadoutDraft::new(platform.clone());
    let mut errors = Vec::new();
    for (slot_id, component_sku, col, row) in placements {
        // Grid placements carry the requested cell; non-grid mounts position
        // through their render anchor and ignore the indices.
        let result = draft.equip_component(catalog, slot_id, component_sku, Some((*col, *row)));
        if let Err(error) = result {
            errors.push(error.to_string());
        }
    }

    let compliance = if policy.is_some() || role.is_some() {
        Some(evaluate_compliance(catalog, &draft, colorway, policy, role))
    } else {
        None
    };

    let mut summary = draft.summary();
    if !errors.is_empty() {
        summary.valid = false;
        summary.errors = errors;
    }
    EvaluateResponse {
        summary,
        compliance,
    }
}

/// Map a `?category=` value onto a [`PlatformCategory`].
fn parse_category(raw: &str) -> Result<PlatformCategory, ApiError> {
    match raw {
        "plate_carrier" => Ok(PlatformCategory::PlateCarrier),
        "belt" => Ok(PlatformCategory::Belt),
        "chest_rig" => Ok(PlatformCategory::ChestRig),
        "lbe" => Ok(PlatformCategory::Lbe),
        other => Err(ApiError::InvalidParam {
            param: "category".to_owned(),
            value: other.to_owned(),
        }),
    }
}

/// Build the list-view summary of a platform.
fn platform_summary(platform: &PlatformSpec) -> PlatformSummary {
    PlatformSummary {
        sku: platform.sku.clone(),
        name: platform.name.clone(),
        category: platform.category,
        slot_count: platform.slots.len(),
    }
}

/// Build one classified component entry.
fn component_entry(
    component: &ComponentSpec,
    adapters: Option<Vec<AdapterSummary>>,
    reason: Option<IncompatibilityReason>,
) -> ComponentCompatEntry {
    ComponentCompatEntry {
        sku: component.sku.clone(),
        name: component.name.clone(),
        kind: component.kind,
        adapters,
        reason,
    }
}
