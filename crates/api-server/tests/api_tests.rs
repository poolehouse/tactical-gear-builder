//! Spec-03 API integration tests: catalog routes, per-slot compatibility
//! discovery, and loadout evaluation, exercised through
//! `tower::ServiceExt::oneshot` (no live socket needed).
//!
//! The catalog is bootstrapped through the in-memory `data-ingest` fallback,
//! so these tests never depend on a pre-generated `catalog_seed.json` file.

use axum::body::{Body, to_bytes};
use axum::http::{Request, StatusCode, header};
use axum::Router;
use serde_json::Value;
use tower::ServiceExt;

use api_server::{AppState, build_router, load_catalog};

const JPC: &str = "crye-precision-jpc-2-0";
const MK5: &str = "spiritus-mk5-placard";
const AXL_AVP: &str = "axl-avp-jpc";
const KYWI: &str = "esstac-kywi-556-double";

// --- helpers -------------------------------------------------------------------

/// Build a test app over the synthesized sample catalog.
fn app() -> Router {
    let catalog = load_catalog(None).expect("catalog bootstrap never fails in tests");
    build_router(AppState::new(catalog))
}

/// Send one request through the router and decode the JSON response.
async fn send(app: &Router, request: Request<Body>) -> (StatusCode, Value) {
    let response = app.clone().oneshot(request).await.expect("router call");
    let status = response.status();
    let bytes = to_bytes(response.into_body(), usize::MAX)
        .await
        .expect("response body");
    let body = serde_json::from_slice(&bytes).expect("response is JSON");
    (status, body)
}

/// Get request to `uri`.
async fn get(app: &Router, uri: &str) -> (StatusCode, Value) {
    send(app, Request::builder().uri(uri).body(Body::empty()).unwrap()).await
}

/// POST request with a JSON body to `uri`.
async fn post(app: &Router, uri: &str, body: Value) -> (StatusCode, Value) {
    let request = Request::builder()
        .method("POST")
        .uri(uri)
        .header(header::CONTENT_TYPE, "application/json")
        .body(Body::from(serde_json::to_vec(&body).unwrap()))
        .unwrap();
    send(app, request).await
}

/// Find the entry with `sku` inside a JSON array of objects.
fn find_entry<'a>(entries: &'a [Value], sku: &str) -> &'a Value {
    entries
        .iter()
        .find(|entry| entry["sku"] == sku)
        .unwrap_or_else(|| panic!("no entry with sku `{sku}` in {entries:?}"))
}

// --- tests -----------------------------------------------------------------------

#[tokio::test]
async fn health_returns_ok() {
    let (status, body) = get(&app(), "/health").await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["status"], "ok");
}

#[tokio::test]
async fn platforms_list_and_category_filter() {
    let app = app();

    // Full catalog: the sample sheets yield 29 platforms.
    let (status, body) = get(&app, "/api/v1/catalog/platforms").await;
    assert_eq!(status, StatusCode::OK);
    let platforms = body.as_array().expect("list response");
    assert_eq!(platforms.len(), 29);
    let jpc = find_entry(platforms, JPC);
    assert_eq!(jpc["category"], "plate_carrier");
    assert_eq!(jpc["slot_count"], 6);

    // Category filter: the belts sheet carries 4 rows.
    let (status, body) = get(&app, "/api/v1/catalog/platforms?category=belt").await;
    assert_eq!(status, StatusCode::OK);
    let belts = body.as_array().expect("filtered list response");
    assert_eq!(belts.len(), 4);
    assert!(belts.iter().all(|belt| belt["category"] == "belt"));
    assert!(belts.iter().any(|belt| belt["sku"] == "aws-smu-belt"));

    // Unknown category is a 400, not a silent empty list.
    let (status, body) = get(&app, "/api/v1/catalog/platforms?category=warship").await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert!(body["error"].as_str().unwrap().contains("category"));
}

#[tokio::test]
async fn platform_details_including_slots() {
    let app = app();

    let (status, body) = get(&app, &format!("/api/v1/catalog/platforms/{JPC}")).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["sku"], JPC);
    assert_eq!(body["category"], "plate_carrier");
    let slots = body["slots"].as_array().expect("slot definitions");
    let front = slots
        .iter()
        .find(|slot| slot["id"] == "front_placard")
        .expect("front_placard slot");
    assert_eq!(front["interface"], "CryeAvsFlap");

    // Unknown platform SKU is a 404.
    let (status, _) = get(&app, "/api/v1/catalog/platforms/no-such-carrier").await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn compatible_components_classify_the_jpc_front_placard_slot() {
    let app = app();
    let uri = format!("/api/v1/catalog/platforms/{JPC}/compatible-components?slot_id=front_placard");

    let (status, body) = get(&app, &uri).await;
    assert_eq!(status, StatusCode::OK);

    // No starter component natively mates with the Crye AVS flap.
    let native = body["native_fit"].as_array().unwrap();
    assert!(native.is_empty(), "native_fit: {native:?}");

    // SwiftClip placards mount through exactly one adapter hop: the AXL AVP.
    let requires_adapter = body["requires_adapter"].as_array().unwrap();
    let mk5 = find_entry(requires_adapter, MK5);
    let adapters = mk5["adapters"].as_array().expect("adapter list");
    assert_eq!(adapters.len(), 1);
    assert_eq!(adapters[0]["sku"], AXL_AVP);

    // PALS-webbing pouches are a different attachment family entirely.
    let incompatible = body["incompatible"].as_array().unwrap();
    let kywi = find_entry(incompatible, KYWI);
    assert!(kywi.get("adapters").is_none(), "incompatible entries carry no adapters");

    // The classification is exhaustive over the catalog.
    let total = native.len() + requires_adapter.len() + incompatible.len();
    assert_eq!(total, 12);

    // Missing slot_id and unknown slot are 400s; unknown platform a 404.
    let (status, _) = get(&app, &format!("/api/v1/catalog/platforms/{JPC}/compatible-components")).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    let (status, _) = get(
        &app,
        &format!("/api/v1/catalog/platforms/{JPC}/compatible-components?slot_id=lasergun"),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    let (status, _) = get(
        &app,
        "/api/v1/catalog/platforms/no-such-carrier/compatible-components?slot_id=front_placard",
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn evaluate_loadout_auto_injects_adapter_and_orders_layers() {
    let app = app();
    let payload = serde_json::json!({
        "platform_sku": JPC,
        "placements": [
            { "slot_id": "front_placard", "component_sku": MK5, "col_idx": 0, "row_idx": 0 },
        ],
    });

    let (status, body) = post(&app, "/api/v1/loadouts/evaluate", payload).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["valid"], true);
    assert_eq!(body["platform_sku"], JPC);

    // Ordered render stack: platform (0) < adapter (10) < placard (20).
    let layers = body["layers_2d"].as_array().unwrap();
    assert_eq!(layers.len(), 3);
    assert_eq!(layers[0]["kind"], "platform");
    assert_eq!(layers[0]["z_index"], 0);
    assert_eq!(layers[1]["kind"], "adapter");
    assert_eq!(layers[1]["sku"], AXL_AVP);
    assert_eq!(layers[1]["z_index"], 10);
    assert_eq!(layers[2]["kind"], "component");
    assert_eq!(layers[2]["sku"], MK5);
    assert_eq!(layers[2]["z_index"], 20);

    // BOM: both the placard and the auto-injected adapter.
    let bom = body["bom"].as_array().unwrap();
    assert_eq!(bom.len(), 2);
    let adapter_line = find_entry(bom, AXL_AVP);
    assert_eq!(adapter_line["is_adapter"], true);
    assert_eq!(adapter_line["quantity"], 1);
    let placard_line = find_entry(bom, MK5);
    assert_eq!(placard_line["is_adapter"], false);
    assert_eq!(placard_line["quantity"], 1);

    assert!(body["errors"].as_array().unwrap().is_empty());
}

#[tokio::test]
async fn evaluate_loadout_rejects_unknown_platform() {
    let app = app();
    let payload = serde_json::json!({
        "platform_sku": "no-such-carrier",
        "placements": [],
    });
    let (status, body) = post(&app, "/api/v1/loadouts/evaluate", payload).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    assert!(body["error"].as_str().unwrap().contains("no-such-carrier"));
}

#[tokio::test]
async fn evaluate_loadout_reports_failed_placements_without_failing() {
    let app = app();

    // Belt with two overlapping KYWI pouches: the second placement collides
    // on the live PALS grid but the response stays 200 with valid=false.
    let payload = serde_json::json!({
        "platform_sku": "aws-smu-belt",
        "placements": [
            { "slot_id": "belt_outer_pals", "component_sku": KYWI, "col_idx": 0, "row_idx": 0 },
            { "slot_id": "belt_outer_pals", "component_sku": KYWI, "col_idx": 1, "row_idx": 0 },
        ],
    });
    let (status, body) = post(&app, "/api/v1/loadouts/evaluate", payload).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["valid"], false);
    let errors = body["errors"].as_array().unwrap();
    assert_eq!(errors.len(), 1);
    assert!(errors[0].as_str().unwrap().contains("PALS"));
    // Only the first pouch made it into the BOM.
    let bom = body["bom"].as_array().unwrap();
    assert_eq!(bom.len(), 1);
    assert_eq!(find_entry(bom, KYWI)["quantity"], 1);

    // Unknown components and slots accumulate as errors too.
    let payload = serde_json::json!({
        "platform_sku": JPC,
        "placements": [
            { "slot_id": "front_placard", "component_sku": "ghost-placard", "col_idx": 0, "row_idx": 0 },
            { "slot_id": "lasergun", "component_sku": MK5, "col_idx": 0, "row_idx": 0 },
        ],
    });
    let (status, body) = post(&app, "/api/v1/loadouts/evaluate", payload).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["valid"], false);
    assert_eq!(body["errors"].as_array().unwrap().len(), 2);
    // No successful placements: platform layer only.
    assert_eq!(body["layers_2d"].as_array().unwrap().len(), 1);
    assert_eq!(body["bom"].as_array().unwrap().len(), 0);
}

#[tokio::test]
async fn evaluate_loadout_mounts_onto_child_slots() {
    let app = app();

    // Mount the MK5 placard, then a KYWI onto its exposed front PALS field,
    // addressed as `<mount_id>.placard_front_pals`.
    let payload = serde_json::json!({
        "platform_sku": JPC,
        "placements": [
            { "slot_id": "front_placard", "component_sku": MK5, "col_idx": 0, "row_idx": 0 },
            { "slot_id": "m2.placard_front_pals", "component_sku": KYWI, "col_idx": 0, "row_idx": 0 },
        ],
    });
    let (status, body) = post(&app, "/api/v1/loadouts/evaluate", payload).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["valid"], true);

    let layers = body["layers_2d"].as_array().unwrap();
    assert_eq!(layers.len(), 4);
    assert_eq!(layers[3]["kind"], "component");
    assert_eq!(layers[3]["sku"], KYWI);
    assert_eq!(layers[3]["z_index"], 30);
    assert_eq!(layers[3]["slot_id"], "m2.placard_front_pals");

    let bom = body["bom"].as_array().unwrap();
    assert_eq!(find_entry(bom, KYWI)["quantity"], 1);
    assert_eq!(find_entry(bom, MK5)["quantity"], 1);
    assert_eq!(find_entry(bom, AXL_AVP)["quantity"], 1);
}

/// The typed rule-code name of one violation/recommendation entry: codes
/// serialize externally tagged (`{"DisallowedColorway": {...}}`).
fn rule_code(entry: &Value) -> String {
    entry["code"]
        .as_object()
        .and_then(|code| code.keys().next().cloned())
        .unwrap_or_else(|| panic!("rule code object missing in {entry:?}"))
}

// --- Spec-04: pricing, compliance, and role presets ------------------------------

#[tokio::test]
async fn evaluate_reports_pricing_rollup_with_adapter_costs() {
    let app = app();
    let payload = serde_json::json!({
        "platform_sku": JPC,
        "placements": [
            { "slot_id": "front_placard", "component_sku": MK5, "col_idx": 0, "row_idx": 0 },
        ],
    });

    let (status, body) = post(&app, "/api/v1/loadouts/evaluate", payload).await;
    assert_eq!(status, StatusCode::OK);

    // Per-line pricing: MSRP, best in-stock offer, savings.
    let placard_line = find_entry(body["bom"].as_array().unwrap(), MK5);
    assert!(placard_line["msrp_cents"].as_u64().unwrap() > 0);
    let offer = placard_line["best_offer"].as_object().expect("in-stock offer");
    assert!(offer["price_cents"].as_u64().unwrap() > 0);
    assert!(!offer["vendor_name"].as_str().unwrap().is_empty());
    assert!(offer["in_stock"].as_bool().unwrap());
    let msrp = placard_line["msrp_cents"].as_u64().unwrap();
    let best = offer["price_cents"].as_u64().unwrap();
    assert_eq!(placard_line["savings_cents"].as_u64().unwrap(), msrp - best);

    // Totals include the platform, the adapter, and the placard, qty-weighted.
    let jpc_msrp = 29_900;
    let total = body["total_msrp_cents"].as_u64().unwrap();
    assert!(total >= jpc_msrp + 6_000 + 14_000, "total must cover platform+adapter+placard: {total}");
    let optimized = body["total_best_price_cents"].as_u64().unwrap();
    assert!(optimized < total, "in-stock offers must beat MSRP: {optimized} vs {total}");
    assert_eq!(body["total_savings_cents"].as_u64().unwrap(), total - optimized);
}

#[tokio::test]
async fn evaluate_without_policy_omits_compliance() {
    let app = app();
    let payload = serde_json::json!({
        "platform_sku": JPC,
        "placements": [],
    });
    let (status, body) = post(&app, "/api/v1/loadouts/evaluate", payload).await;
    assert_eq!(status, StatusCode::OK);
    assert!(body.get("compliance").is_none(), "no policy or role named -> no report");
}

#[tokio::test]
async fn evaluate_black_jpc_violates_ar670_1_and_multicam_passes() {
    let app = app();

    // Bare platform in Black under AR 670-1: disallowed colorway, missing
    // IFAK, insufficient magazine carriage — all under HTTP 200.
    let payload = serde_json::json!({
        "platform_sku": JPC,
        "placements": [],
        "colorway": "black",
        "regulation_policy": "us_army_ar670_1",
    });
    let (status, body) = post(&app, "/api/v1/loadouts/evaluate", payload).await;
    assert_eq!(status, StatusCode::OK);
    let compliance = &body["compliance"];
    assert_eq!(compliance["compliant"], false);
    let codes: Vec<String> = compliance["violations"]
        .as_array()
        .unwrap()
        .iter()
        .map(rule_code)
        .collect();
    assert!(codes.contains(&"DisallowedColorway".to_owned()), "codes: {codes:?}");
    assert!(codes.contains(&"MissingCapability".to_owned()), "codes: {codes:?}");
    assert!(codes.contains(&"InsufficientRifleMagCapacity".to_owned()), "codes: {codes:?}");
    // Suggestions point at mountable catalog SKUs.
    let ifak_violation = compliance["violations"]
        .as_array()
        .unwrap()
        .iter()
        .find(|v| rule_code(v) == "MissingCapability")
        .expect("IFAK violation");
    let suggested = ifak_violation["suggested_skus"].as_array().unwrap();
    assert!(!suggested.is_empty(), "IFAK suggestions: {suggested:?}");

    // Multicam with IFAK and magazine carriage equipped: compliant.
    let payload = serde_json::json!({
        "platform_sku": JPC,
        "placements": [
            { "slot_id": "front_placard", "component_sku": MK5, "col_idx": 0, "row_idx": 0 },
            { "slot_id": "m2.placard_front_pals", "component_sku": KYWI, "col_idx": 0, "row_idx": 0 },
            { "slot_id": "m2.placard_front_pals", "component_sku": KYWI, "col_idx": 2, "row_idx": 0 },
            { "slot_id": "dangler_slot", "component_sku": "bfg-micro-trauma-now", "col_idx": 0, "row_idx": 0 },
        ],
        "colorway": "multicam",
        "regulation_policy": "us_army_ar670_1",
    });
    let (status, body) = post(&app, "/api/v1/loadouts/evaluate", payload).await;
    assert_eq!(status, StatusCode::OK);
    let compliance = &body["compliance"];
    assert_eq!(compliance["compliant"], true, "violations: {compliance:?}");
    assert!(compliance["violations"].as_array().unwrap().is_empty());
}

#[tokio::test]
async fn sniper_dm_role_warns_on_stacked_placard_pouches() {
    let app = app();

    // A KYWI double stacked on the MK5 placard's front PALS field under the
    // Sniper/DM role: non-blocking prone-profile recommendation.
    let payload = serde_json::json!({
        "platform_sku": JPC,
        "placements": [
            { "slot_id": "front_placard", "component_sku": MK5, "col_idx": 0, "row_idx": 0 },
            { "slot_id": "m2.placard_front_pals", "component_sku": KYWI, "col_idx": 0, "row_idx": 0 },
        ],
        "operational_role": "sniper_dm",
    });
    let (status, body) = post(&app, "/api/v1/loadouts/evaluate", payload).await;
    assert_eq!(status, StatusCode::OK);
    let compliance = &body["compliance"];
    assert_eq!(compliance["compliant"], true, "recommendations never block: {compliance:?}");
    let recs = compliance["recommendations"].as_array().unwrap();
    let codes: Vec<String> = recs.iter().map(rule_code).collect();
    assert!(
        codes.contains(&"ProneProfileStacking".to_owned()),
        "prone stacking expected, got: {codes:?}"
    );
    let stacking = recs.iter().find(|rec| rule_code(rec) == "ProneProfileStacking").unwrap();
    assert_eq!(stacking["code"]["ProneProfileStacking"]["slot_id"], "m2.placard_front_pals");
}

#[tokio::test]
async fn presets_endpoint_lists_all_six_curated_presets() {
    let (status, body) = get(&app(), "/api/v1/catalog/presets").await;
    assert_eq!(status, StatusCode::OK);
    let presets = body.as_array().expect("preset list");
    assert_eq!(presets.len(), 6);
    let ids: Vec<&str> = presets.iter().map(|p| p["preset_id"].as_str().unwrap()).collect();
    assert_eq!(ids, vec!["breacher", "jtac", "drone-operator", "sniper-dm", "recce", "le-patrol"]);

    // Summaries carry the validation context and placement counts.
    let jtac = presets.iter().find(|p| p["preset_id"] == "jtac").unwrap();
    assert_eq!(jtac["role"], "jtac");
    assert_eq!(jtac["policy"], "us_army_ar670_1");
    assert_eq!(jtac["platform_sku"], JPC);
    assert_eq!(jtac["default_colorway"], "multicam");
    assert_eq!(jtac["placement_count"], 5);
    let le = presets.iter().find(|p| p["preset_id"] == "le-patrol").unwrap();
    assert!(le["role"].is_null(), "LE patrol is policy-only");
    assert_eq!(le["default_colorway"], "black");
}

#[tokio::test]
async fn from_preset_instantiates_a_compliant_valid_build() {
    let app = app();

    let (status, body) = post(&app, "/api/v1/loadouts/from-preset/jtac", serde_json::json!({})).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["preset_id"], "jtac");
    assert_eq!(body["platform_sku"], JPC);
    assert_eq!(body["valid"], true);
    assert!(body["errors"].as_array().unwrap().is_empty());

    // The build is compliant under its own policy and role.
    let compliance = &body["compliance"];
    assert_eq!(compliance["compliant"], true, "violations: {compliance:?}");

    // The adapter cost shows up in the priced BOM.
    let bom = body["bom"].as_array().unwrap();
    let adapter_line = find_entry(bom, AXL_AVP);
    assert_eq!(adapter_line["is_adapter"], true);
    assert!(adapter_line["msrp_cents"].as_u64().unwrap() > 0);

    // Placements echo back so a client can continue editing.
    let placements = body["placements"].as_array().unwrap();
    assert_eq!(placements.len(), 5);
    assert_eq!(placements[0]["slot_id"], "front_placard");
    assert_eq!(placements[0]["component_sku"], MK5);

    // Unknown preset ids are 404s.
    let (status, body) = post(&app, "/api/v1/loadouts/from-preset/ghost", serde_json::json!({})).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    assert!(body["error"].as_str().unwrap().contains("ghost"));
}
