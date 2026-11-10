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
//! - `POST /api/v1/loadouts` — persist a validated loadout document
//! - `GET  /api/v1/loadouts` — list persisted loadout metadata
//! - `GET  /api/v1/loadouts/{id}` — persisted loadout + live evaluation
//! - `PATCH /api/v1/loadouts/{id}` — declarative update behind an
//!   optimistic-lock `expected_version` guard
//! - `DELETE /api/v1/loadouts/{id}`
//! - `GET  /api/v1/loadouts/{id}/evaluate?catalog_version=` — evaluate a
//!   stored loadout against its pinned revision or an explicit newer one

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
use store::{LoadoutMetadata, PersistedLoadout, Store, StoreError, StoredPlacement};
use thiserror::Error;
use tower_http::cors::CorsLayer;
use tower_http::trace::TraceLayer;
use uuid::Uuid;

/// Shared application state: the immutable product catalog plus, when the
/// server booted with a database, the persistence layer.
#[derive(Clone)]
pub struct AppState {
    catalog: Arc<Catalog>,
    store: Option<Arc<Store>>,
}

impl AppState {
    /// Wrap a catalog in shared state without persistence (the stateless
    /// routes only; persisted-loadout routes answer 503).
    pub fn new(catalog: Catalog) -> Self {
        Self {
            catalog: Arc::new(catalog),
            store: None,
        }
    }

    /// Wrap a catalog and a migrated, seeded [`Store`] in shared state.
    /// Accepts either an owned store or an already-shared one.
    pub fn with_store(catalog: Catalog, store: impl Into<Arc<Store>>) -> Self {
        Self {
            catalog: Arc::new(catalog),
            store: Some(store.into()),
        }
    }

    /// The shared catalog.
    pub fn catalog(&self) -> &Catalog {
        &self.catalog
    }

    /// The persistence layer, when one is configured.
    pub fn store(&self) -> Option<&Arc<Store>> {
        self.store.as_ref()
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
        .route("/api/v1/loadouts", post(create_loadout).get(list_loadouts))
        .route(
            "/api/v1/loadouts/{id}",
            get(get_loadout).patch(update_loadout).delete(delete_loadout),
        )
        .route("/api/v1/loadouts/{id}/evaluate", get(evaluate_stored_loadout))
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
    /// The persistence layer is not configured on this server (503).
    #[error("persistence is not configured on this server")]
    PersistenceUnavailable,
    /// No loadout with the requested id exists (404).
    #[error("unknown loadout id `{0}`")]
    UnknownLoadout(Uuid),
    /// No catalog revision with the requested version exists (404).
    #[error("unknown catalog revision {0}")]
    UnknownCatalogRevision(i64),
    /// The caller's expected version is stale (409).
    #[error("version_conflict")]
    VersionConflict {
        /// The version actually stored in the row.
        current_version: i64,
    },
    /// A stored record failed `core-engine` validation on read (R2); the
    /// row is never silently coerced (500).
    #[error("corrupt_or_invalid_record")]
    CorruptOrInvalidRecord {
        /// What was read and why it failed.
        detail: String,
    },
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let status = match &self {
            ApiError::UnknownPlatform(_)
            | ApiError::UnknownPreset(_)
            | ApiError::UnknownLoadout(_)
            | ApiError::UnknownCatalogRevision(_) => StatusCode::NOT_FOUND,
            ApiError::MissingParam(_) | ApiError::InvalidParam { .. } => StatusCode::BAD_REQUEST,
            ApiError::InvalidBody(_) => StatusCode::BAD_REQUEST,
            ApiError::Catalog(_) | ApiError::CorruptOrInvalidRecord { .. } => {
                StatusCode::INTERNAL_SERVER_ERROR
            }
            ApiError::PersistenceUnavailable => StatusCode::SERVICE_UNAVAILABLE,
            ApiError::VersionConflict { .. } => StatusCode::CONFLICT,
        };
        let body = match &self {
            ApiError::VersionConflict { current_version } => serde_json::json!({
                "error": "version_conflict",
                "current_version": current_version,
            }),
            ApiError::CorruptOrInvalidRecord { detail } => serde_json::json!({
                "error": "corrupt_or_invalid_record",
                "detail": detail,
            }),
            _ => serde_json::json!({ "error": self.to_string() }),
        };
        (status, Json(body)).into_response()
    }
}

/// Map a [`StoreError`] onto its transport-level `ApiError`.
fn store_error(error: StoreError) -> ApiError {
    match error {
        StoreError::UnknownLoadout(id) => ApiError::UnknownLoadout(id),
        StoreError::UnknownCatalogRevision(version) => ApiError::UnknownCatalogRevision(version),
        StoreError::VersionConflict { current_version } => ApiError::VersionConflict {
            current_version,
        },
        StoreError::CorruptOrInvalidRecord { detail } => ApiError::CorruptOrInvalidRecord {
            detail,
        },
        StoreError::Database(detail) => ApiError::Catalog(format!("database error: {detail}")),
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

// --- Spec-05: persisted loadouts ----------------------------------------------------

/// `POST /api/v1/loadouts` request body — the same declarative shape as
/// `/evaluate`, but persisted (when valid) as a loadout document.
#[derive(Debug, Clone, Deserialize)]
pub struct CreateLoadoutRequest {
    /// SKU of the base platform.
    pub platform_sku: String,
    /// Mounts to apply, in order.
    #[serde(default)]
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

/// `PATCH /api/v1/loadouts/{id}` request body. Absent fields keep their
/// current values; `expected_version` must match the stored version.
#[derive(Debug, Clone, Deserialize)]
pub struct UpdateLoadoutRequest {
    /// The version the client based this edit on (optimistic lock).
    pub expected_version: i64,
    /// Replacement colorway, when changing it.
    #[serde(default)]
    pub colorway: Option<Colorway>,
    /// Replacement regulation policy, when changing it.
    #[serde(default)]
    pub regulation_policy: Option<RegulationPolicy>,
    /// Replacement operational role, when changing it.
    #[serde(default)]
    pub operational_role: Option<OperationalRole>,
    /// Replacement placement list, when changing the build.
    #[serde(default)]
    pub placements: Option<Vec<PlacementRequest>>,
}

/// `POST`/`PATCH /api/v1/loadouts...` response: the R4/11A contract —
/// semantic engine failures come back as HTTP 200 with `persisted: false`,
/// `valid: false`, and the structured `errors[]` inside the summary.
#[derive(Debug, Clone, Serialize)]
pub struct LoadoutWriteResponse {
    /// Whether the document was written to SQLite.
    pub persisted: bool,
    /// Whether every placement passed engine validation.
    pub valid: bool,
    /// The created/updated loadout id (persisted responses only).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<Uuid>,
    /// The document version after the write (persisted responses only).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub version: Option<i64>,
    /// The pinned catalog revision (persisted responses only).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub catalog_version: Option<i64>,
    /// The full evaluation summary (with `errors[]` when invalid).
    pub summary: EvaluateResponse,
}

/// `GET /api/v1/loadouts` response.
#[derive(Debug, Clone, Serialize)]
pub struct LoadoutListResponse {
    /// Persisted loadout metadata, newest update first.
    pub loadouts: Vec<LoadoutMetadata>,
    /// Page size actually applied.
    pub limit: u32,
    /// Page offset actually applied.
    pub offset: u32,
}

/// `GET /api/v1/loadouts` query parameters.
#[derive(Debug, Clone, Default, Deserialize)]
struct ListQuery {
    /// Page size (default 50, capped at 200).
    limit: Option<u32>,
    /// Page offset (default 0).
    offset: Option<u32>,
}

/// `GET /api/v1/loadouts/{id}` response: the stored document plus its live
/// evaluation against the pinned catalog revision.
#[derive(Debug, Clone, Serialize)]
pub struct LoadoutDetailResponse {
    /// Persisted row metadata.
    #[serde(flatten)]
    pub metadata: LoadoutMetadata,
    /// The stored placement list.
    pub placements: Vec<StoredPlacement>,
    /// The fail-closed replay evaluation (R2).
    pub summary: EvaluateResponse,
}

/// `GET /api/v1/loadouts/{id}/evaluate` response.
#[derive(Debug, Clone, Serialize)]
pub struct LoadoutEvaluateResponse {
    /// The evaluated loadout id.
    pub id: Uuid,
    /// The document version evaluated.
    pub version: i64,
    /// The catalog revision evaluated against.
    pub catalog_version: i64,
    /// The evaluation summary.
    pub summary: EvaluateResponse,
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

// --- Spec-05: persisted loadout handlers --------------------------------------------

/// Resolve the store, or explain that this server booted stateless.
fn require_store(state: &AppState) -> Result<&Arc<Store>, ApiError> {
    state.store().ok_or(ApiError::PersistenceUnavailable)
}

/// `POST /api/v1/loadouts` — evaluate the requested placements through the
/// engine first (11A): valid builds persist as revision-pinned documents
/// (201), invalid builds are reported under HTTP 200 with
/// `persisted: false` and never written (R4).
async fn create_loadout(
    State(state): State<AppState>,
    Json(payload): Json<CreateLoadoutRequest>,
) -> Result<(StatusCode, Json<LoadoutWriteResponse>), ApiError> {
    let store = require_store(&state)?;
    let catalog_version = store.active_catalog_version().await.map_err(store_error)?;
    let catalog = store
        .load_catalog_revision(catalog_version)
        .await
        .map_err(store_error)?;
    let platform = catalog
        .platform(&payload.platform_sku)
        .cloned()
        .ok_or_else(|| ApiError::UnknownPlatform(payload.platform_sku.clone()))?;
    let colorway = payload.colorway.unwrap_or(platform.default_colorway);
    let placements = to_stored(&payload.placements);

    let evaluation = evaluate_placements(
        &catalog,
        &platform,
        &placement_tuples(&placements),
        colorway,
        payload.regulation_policy,
        payload.operational_role,
    );
    if !evaluation.summary.valid {
        return Ok((
            StatusCode::OK,
            Json(not_persisted(evaluation)),
        ));
    }

    let persisted = store
        .create_loadout(
            &payload.platform_sku,
            catalog_version,
            colorway,
            payload.regulation_policy,
            payload.operational_role,
            &placements,
        )
        .await
        .map_err(store_error)?;
    Ok((
        StatusCode::CREATED,
        Json(LoadoutWriteResponse {
            persisted: true,
            valid: true,
            id: Some(persisted.metadata.id),
            version: Some(persisted.metadata.version),
            catalog_version: Some(persisted.metadata.catalog_version),
            summary: evaluation,
        }),
    ))
}

/// `GET /api/v1/loadouts` — persisted loadout metadata, newest update
/// first, paginated through `?limit=&offset=`.
async fn list_loadouts(
    State(state): State<AppState>,
    Query(params): Query<ListQuery>,
) -> Result<Json<LoadoutListResponse>, ApiError> {
    let store = require_store(&state)?;
    let limit = params.limit.unwrap_or(50).min(200);
    let offset = params.offset.unwrap_or(0);
    let loadouts = store
        .list_loadouts(limit, offset)
        .await
        .map_err(store_error)?;
    Ok(Json(LoadoutListResponse {
        loadouts,
        limit,
        offset,
    }))
}

/// `GET /api/v1/loadouts/{id}` — the stored document plus its live,
/// fail-closed evaluation against the pinned revision (R2). Unknown ids
/// are 404s; a row that no longer replays cleanly is a structured 500,
/// never a coerced `valid: false`.
async fn get_loadout(
    State(state): State<AppState>,
    PathParam(id): PathParam<Uuid>,
) -> Result<Json<LoadoutDetailResponse>, ApiError> {
    let store = require_store(&state)?;
    let loadout = fetch_loadout(store, id).await?;
    Ok(Json(evaluate_pinned_loadout(store, &loadout).await?))
}

/// `PATCH /api/v1/loadouts/{id}` — declarative update behind the
/// optimistic-lock guard. Transport failures first (`404` unknown id,
/// `409` stale `expected_version`), then the same evaluate-before-write
/// contract as `POST` (11A): an invalid merged build is a 200 with
/// `persisted: false` and the row is left untouched.
async fn update_loadout(
    State(state): State<AppState>,
    PathParam(id): PathParam<Uuid>,
    Json(payload): Json<UpdateLoadoutRequest>,
) -> Result<Json<LoadoutWriteResponse>, ApiError> {
    let store = require_store(&state)?;
    let loadout = fetch_loadout(store, id).await?;

    // Transport check first: a stale edit base must not be answered with
    // a semantic result computed against someone else's row state.
    if payload.expected_version != loadout.metadata.version {
        return Err(ApiError::VersionConflict {
            current_version: loadout.metadata.version,
        });
    }

    // Absent fields keep their current values.
    let colorway = payload.colorway.unwrap_or(loadout.metadata.colorway);
    let regulation_policy = payload
        .regulation_policy
        .or(loadout.metadata.regulation_policy);
    let operational_role = payload
        .operational_role
        .or(loadout.metadata.operational_role);
    let placements = payload
        .placements
        .as_ref()
        .map(|placements| to_stored(placements))
        .unwrap_or_else(|| loadout.placements.clone());

    let catalog = store
        .load_catalog_revision(loadout.metadata.catalog_version)
        .await
        .map_err(store_error)?;
    let platform = pinned_platform(&catalog, &loadout)?;
    let evaluation = evaluate_placements(
        &catalog,
        &platform,
        &placement_tuples(&placements),
        colorway,
        regulation_policy,
        operational_role,
    );
    if !evaluation.summary.valid {
        return Ok(Json(not_persisted(evaluation)));
    }

    let updated = store
        .update_loadout(
            id,
            payload.expected_version,
            colorway,
            regulation_policy,
            operational_role,
            &placements,
        )
        .await
        .map_err(store_error)?;
    Ok(Json(LoadoutWriteResponse {
        persisted: true,
        valid: true,
        id: Some(updated.metadata.id),
        version: Some(updated.metadata.version),
        catalog_version: Some(updated.metadata.catalog_version),
        summary: evaluation,
    }))
}

/// `DELETE /api/v1/loadouts/{id}` — removes the document and its audit
/// trail; `204` on success, `404` for unknown ids.
async fn delete_loadout(
    State(state): State<AppState>,
    PathParam(id): PathParam<Uuid>,
) -> Result<StatusCode, ApiError> {
    let store = require_store(&state)?;
    if store.delete_loadout(id).await.map_err(store_error)? {
        Ok(StatusCode::NO_CONTENT)
    } else {
        Err(ApiError::UnknownLoadout(id))
    }
}

/// `GET /api/v1/loadouts/{id}/evaluate?catalog_version=` — evaluates the
/// stored loadout against its pinned revision (fail-closed per R2) or an
/// explicitly named revision. A cross-revision evaluation is a semantic
/// result — the build legitimately may not fit a different catalog — and
/// is audited as a `re_evaluated` event.
async fn evaluate_stored_loadout(
    State(state): State<AppState>,
    PathParam(id): PathParam<Uuid>,
    Query(params): Query<HashMap<String, String>>,
) -> Result<Json<LoadoutEvaluateResponse>, ApiError> {
    let store = require_store(&state)?;
    let loadout = fetch_loadout(store, id).await?;

    let requested = match params.get("catalog_version") {
        None => loadout.metadata.catalog_version,
        Some(raw) => raw
            .parse::<i64>()
            .map_err(|_| ApiError::InvalidParam {
                param: "catalog_version".to_owned(),
                value: raw.clone(),
            })?,
    };

    if requested == loadout.metadata.catalog_version {
        let detail = evaluate_pinned_loadout(store, &loadout).await?;
        return Ok(Json(LoadoutEvaluateResponse {
            id,
            version: detail.metadata.version,
            catalog_version: requested,
            summary: detail.summary,
        }));
    }

    // Cross-revision evaluation: semantic result (R4), audited.
    let catalog = store
        .load_catalog_revision(requested)
        .await
        .map_err(store_error)?;
    let platform = catalog
        .platform(&loadout.metadata.platform_sku)
        .cloned()
        .ok_or_else(|| ApiError::CorruptOrInvalidRecord {
            detail: format!(
                "loadout {id} references platform `{}` absent from revision {requested}",
                loadout.metadata.platform_sku
            ),
        })?;
    let evaluation = evaluate_placements(
        &catalog,
        &platform,
        &placement_tuples(&loadout.placements),
        loadout.metadata.colorway,
        loadout.metadata.regulation_policy,
        loadout.metadata.operational_role,
    );
    store
        .record_re_evaluation(id, requested)
        .await
        .map_err(store_error)?;
    Ok(Json(LoadoutEvaluateResponse {
        id,
        version: loadout.metadata.version,
        catalog_version: requested,
        summary: evaluation,
    }))
}

// --- helpers ----------------------------------------------------------------------

/// Fetch a loadout or surface its 404.
async fn fetch_loadout(store: &Store, id: Uuid) -> Result<PersistedLoadout, ApiError> {
    store
        .get_loadout(id)
        .await
        .map_err(store_error)?
        .ok_or(ApiError::UnknownLoadout(id))
}

/// Look up the loadout's platform inside its pinned revision; a missing
/// platform in a pinned document is corruption (R2), not a 404 — the loadout
/// itself exists.
fn pinned_platform(catalog: &Catalog, loadout: &PersistedLoadout) -> Result<PlatformSpec, ApiError> {
    catalog
        .platform(&loadout.metadata.platform_sku)
        .cloned()
        .ok_or_else(|| ApiError::CorruptOrInvalidRecord {
            detail: format!(
                "loadout {} references unknown platform `{}` in revision {}",
                loadout.metadata.id, loadout.metadata.platform_sku, loadout.metadata.catalog_version
            ),
        })
}

/// Replay a stored loadout against its pinned revision, fail-closed (R2):
/// the document was only ever written after passing this exact evaluation,
/// so a row that no longer replays cleanly has been corrupted out-of-band
/// and surfaces as a structured 500.
async fn evaluate_pinned_loadout(
    store: &Store,
    loadout: &PersistedLoadout,
) -> Result<LoadoutDetailResponse, ApiError> {
    let catalog = store
        .load_catalog_revision(loadout.metadata.catalog_version)
        .await
        .map_err(store_error)?;
    let platform = pinned_platform(&catalog, loadout)?;
    let evaluation = evaluate_placements(
        &catalog,
        &platform,
        &placement_tuples(&loadout.placements),
        loadout.metadata.colorway,
        loadout.metadata.regulation_policy,
        loadout.metadata.operational_role,
    );
    if !evaluation.summary.valid {
        return Err(ApiError::CorruptOrInvalidRecord {
            detail: format!(
                "stored loadout {} no longer evaluates cleanly against revision {}: {:?}",
                loadout.metadata.id, loadout.metadata.catalog_version, evaluation.summary.errors
            ),
        });
    }
    Ok(LoadoutDetailResponse {
        metadata: loadout.metadata.clone(),
        placements: loadout.placements.clone(),
        summary: evaluation,
    })
}

/// The R4/11A semantic-failure response body: nothing was written.
fn not_persisted(evaluation: EvaluateResponse) -> LoadoutWriteResponse {
    LoadoutWriteResponse {
        persisted: false,
        valid: false,
        id: None,
        version: None,
        catalog_version: None,
        summary: evaluation,
    }
}

/// Convert API placement requests into their canonical stored form.
fn to_stored(requests: &[PlacementRequest]) -> Vec<StoredPlacement> {
    requests
        .iter()
        .map(|p| StoredPlacement {
            slot_id: p.slot_id.clone(),
            component_sku: p.component_sku.clone(),
            col_idx: p.col_idx,
            row_idx: p.row_idx,
        })
        .collect()
}

/// Expand stored placements into the tuple shape `evaluate_placements`
/// consumes.
fn placement_tuples(placements: &[StoredPlacement]) -> Vec<(String, String, u8, u8)> {
    placements
        .iter()
        .map(|p| (p.slot_id.clone(), p.component_sku.clone(), p.col_idx, p.row_idx))
        .collect()
}

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
