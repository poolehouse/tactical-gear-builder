//! Spec-05 API integration tests: persisted loadout CRUD, optimistic-lock
//! 409 conflicts, the R4/11A semantic-vs-transport contract, and the
//! pinned/cross-revision evaluate route — exercised through
//! `tower::ServiceExt::oneshot` against an isolated temporary SQLite file.
//!
//! The stateless routes keep their own test file (`api_tests.rs`); these
//! tests only cover the additive persisted-loadout surface.

use axum::body::{Body, to_bytes};
use axum::http::{Request, StatusCode, header};
use axum::Router;
use serde_json::Value;
use std::sync::Arc;
use store::Store;
use tower::ServiceExt;

use api_server::{AppState, build_router, load_catalog};

const JPC: &str = "crye-precision-jpc-2-0";
const MK5: &str = "spiritus-mk5-placard";
const AXL_AVP: &str = "axl-avp-jpc";
const KYWI: &str = "esstac-kywi-556-double";
const BELT: &str = "aws-smu-belt";

// --- helpers -------------------------------------------------------------------

/// A valid JPC 2.0 build: MK5 placard (through the AXL AVP adapter) with a
/// KYWI pouch on its front PALS field.
fn valid_build() -> Value {
    serde_json::json!({
        "platform_sku": JPC,
        "placements": [
            { "slot_id": "front_placard", "component_sku": MK5, "col_idx": 0, "row_idx": 0 },
            { "slot_id": "m2.placard_front_pals", "component_sku": KYWI, "col_idx": 0, "row_idx": 0 },
        ],
        "colorway": "multicam",
    })
}

/// Build a router wired to an isolated, migrated, seeded temp SQLite file.
/// Returns the shared store handle for audit-trail assertions.
async fn persisted_app() -> (Router, Arc<Store>, tempfile::TempDir) {
    let catalog = load_catalog(None).expect("catalog bootstrap never fails in tests");
    let dir = tempfile::tempdir().expect("temp dir");
    let url = format!("sqlite://{}?mode=rwc", dir.path().join("api-test.db").display());
    let store = Arc::new(Store::connect_and_migrate(&url).await.expect("connect + migrate"));
    let seeded = store.ensure_seeded(&catalog).await.expect("seed");
    assert_eq!(seeded, 1);
    let app = build_router(AppState::with_store(catalog, store.clone()));
    (app, store, dir)
}

/// Send one request through the router and decode the JSON response (an
/// empty body, as on 204, decodes to `Value::Null`; non-JSON bodies such
/// as axum's plain-text extractor rejections decode to a string).
async fn send(app: &Router, request: Request<Body>) -> (StatusCode, Value) {
    let response = app.clone().oneshot(request).await.expect("router call");
    let status = response.status();
    let bytes = to_bytes(response.into_body(), usize::MAX)
        .await
        .expect("response body");
    let body = if bytes.is_empty() {
        Value::Null
    } else {
        serde_json::from_slice(&bytes).unwrap_or_else(|_| {
            Value::String(String::from_utf8_lossy(&bytes).into_owned())
        })
    };
    (status, body)
}

async fn get(app: &Router, uri: &str) -> (StatusCode, Value) {
    send(app, Request::builder().uri(uri).body(Body::empty()).unwrap()).await
}

async fn request_with_body(app: &Router, method: &str, uri: &str, body: Value) -> (StatusCode, Value) {
    let request = Request::builder()
        .method(method)
        .uri(uri)
        .header(header::CONTENT_TYPE, "application/json")
        .body(Body::from(serde_json::to_vec(&body).unwrap()))
        .unwrap();
    send(app, request).await
}

async fn post(app: &Router, uri: &str, body: Value) -> (StatusCode, Value) {
    request_with_body(app, "POST", uri, body).await
}

async fn patch(app: &Router, uri: &str, body: Value) -> (StatusCode, Value) {
    request_with_body(app, "PATCH", uri, body).await
}

async fn delete(app: &Router, uri: &str) -> (StatusCode, Value) {
    send(app, Request::builder().method("DELETE").uri(uri).body(Body::empty()).unwrap()).await
}

/// POST a valid build and return its id.
async fn create_valid(app: &Router) -> String {
    let (status, body) = post(app, "/api/v1/loadouts", valid_build()).await;
    assert_eq!(status, StatusCode::CREATED, "body: {body:?}");
    assert_eq!(body["persisted"], true);
    assert_eq!(body["valid"], true);
    assert_eq!(body["version"], 1);
    assert_eq!(body["catalog_version"], 1);
    body["id"].as_str().expect("loadout id").to_owned()
}

/// Audit-trail event types for one loadout, in order.
async fn event_types(store: &Arc<Store>, id: &str) -> Vec<String> {
    let rows: Vec<(String,)> = sqlx::query_as(
        "SELECT event_type FROM loadout_events WHERE loadout_id = ? ORDER BY event_id",
    )
    .bind(id)
    .fetch_all(store.pool())
    .await
    .expect("events");
    rows.into_iter().map(|(event_type,)| event_type).collect()
}

// --- gate 3: full CRUD + audit trail ---------------------------------------------

#[tokio::test]
async fn full_crud_round_trip_with_audit_trail() {
    let (app, store, _dir) = persisted_app().await;

    // POST: persisted as version 1, pinned to revision 1, fully evaluated.
    let (status, body) = post(&app, "/api/v1/loadouts", valid_build()).await;
    assert_eq!(status, StatusCode::CREATED, "body: {body:?}");
    assert_eq!(body["persisted"], true);
    assert_eq!(body["valid"], true);
    assert_eq!(body["version"], 1);
    assert_eq!(body["catalog_version"], 1);
    let id = body["id"].as_str().expect("id").to_owned();
    uuid::Uuid::parse_str(&id).expect("id is a UUID");

    let summary = &body["summary"];
    assert_eq!(summary["platform_sku"], JPC);
    assert_eq!(summary["valid"], true);
    assert!(summary["errors"].as_array().unwrap().is_empty());
    let layers = summary["layers_2d"].as_array().unwrap();
    assert_eq!(layers.len(), 4, "platform + adapter + placard + pouch");
    assert_eq!(layers[1]["sku"], AXL_AVP);
    let bom_skus: Vec<&str> =
        summary["bom"].as_array().unwrap().iter().map(|l| l["sku"].as_str().unwrap()).collect();
    assert!(bom_skus.contains(&MK5) && bom_skus.contains(&KYWI) && bom_skus.contains(&AXL_AVP));
    assert!(summary["total_msrp_cents"].as_u64().unwrap() > 0);

    // GET list shows the metadata.
    let (status, body) = get(&app, "/api/v1/loadouts").await;
    assert_eq!(status, StatusCode::OK);
    let listed = body["loadouts"].as_array().expect("loadouts array");
    assert_eq!(listed.len(), 1);
    assert_eq!(listed[0]["id"], id);
    assert_eq!(listed[0]["version"], 1);
    assert_eq!(listed[0]["platform_sku"], JPC);
    assert_eq!(listed[0]["colorway"], "multicam");

    // GET detail: metadata + stored placements + live evaluation.
    let (status, body) = get(&app, &format!("/api/v1/loadouts/{id}")).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["id"], id);
    assert_eq!(body["version"], 1);
    assert_eq!(body["placements"].as_array().unwrap().len(), 2);
    assert_eq!(body["summary"]["valid"], true);
    assert_eq!(body["summary"]["layers_2d"].as_array().unwrap().len(), 4);

    // PATCH: declarative update, version 1 -> 2.
    let (status, body) = patch(
        &app,
        &format!("/api/v1/loadouts/{id}"),
        serde_json::json!({
            "expected_version": 1,
            "colorway": "coyote_brown",
            "placements": [
                { "slot_id": "front_placard", "component_sku": MK5, "col_idx": 0, "row_idx": 0 },
            ],
        }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "body: {body:?}");
    assert_eq!(body["persisted"], true);
    assert_eq!(body["valid"], true);
    assert_eq!(body["version"], 2);
    assert_eq!(body["summary"]["valid"], true);

    // The row carries the update (colorway + trimmed build).
    let (status, detail) = get(&app, &format!("/api/v1/loadouts/{id}")).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(detail["colorway"], "coyote_brown");
    assert_eq!(detail["version"], 2);
    assert_eq!(detail["placements"].as_array().unwrap().len(), 1);
    // Placard plus its auto-injected adapter bridge.
    assert_eq!(detail["summary"]["bom"].as_array().unwrap().len(), 2);

    // Audit trail: created + updated records exist.
    let events = event_types(&store, &id).await;
    assert_eq!(events, vec!["created".to_owned(), "updated".to_owned()]);

    // DELETE: 204 now, 404 afterwards (and for the detail route).
    let (status, _) = delete(&app, &format!("/api/v1/loadouts/{id}")).await;
    assert_eq!(status, StatusCode::NO_CONTENT);
    let (status, body) = get(&app, &format!("/api/v1/loadouts/{id}")).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    assert!(body["error"].as_str().unwrap().contains("unknown loadout"));
    let (status, _) = delete(&app, &format!("/api/v1/loadouts/{id}")).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    let (status, body) = get(&app, "/api/v1/loadouts").await;
    assert_eq!(status, StatusCode::OK);
    assert!(body["loadouts"].as_array().unwrap().is_empty());
}

// --- gate 4: optimistic concurrency ----------------------------------------------

#[tokio::test]
async fn stale_expected_version_conflicts_with_409() {
    let (app, _store, _dir) = persisted_app().await;
    let id = create_valid(&app).await;

    // First writer moves the row to version 2.
    let (status, body) =
        patch(&app, &format!("/api/v1/loadouts/{id}"), serde_json::json!({ "expected_version": 1 })).await;
    assert_eq!(status, StatusCode::OK, "body: {body:?}");
    assert_eq!(body["persisted"], true);
    assert_eq!(body["version"], 2);

    // A second writer still presenting version 1 gets a 409 conflict.
    let (status, body) =
        patch(&app, &format!("/api/v1/loadouts/{id}"), serde_json::json!({ "expected_version": 1 })).await;
    assert_eq!(status, StatusCode::CONFLICT, "body: {body:?}");
    assert_eq!(body["error"], "version_conflict");
    assert_eq!(body["current_version"], 2);

    // The row was not mutated by the conflicting attempt.
    let (status, detail) = get(&app, &format!("/api/v1/loadouts/{id}")).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(detail["version"], 2);
    assert_eq!(detail["placements"].as_array().unwrap().len(), 2);
}

// --- gate 5: semantic vs. transport separation (R4 + 11A) -------------------------

#[tokio::test]
async fn semantic_placement_errors_are_200_and_never_persisted() {
    let (app, _store, _dir) = persisted_app().await;

    // Two KYWI pouches colliding on the belt's PALS grid: a semantic engine
    // error, so HTTP 200 with valid=false, persisted=false, errors[].
    let colliding = serde_json::json!({
        "platform_sku": BELT,
        "placements": [
            { "slot_id": "belt_outer_pals", "component_sku": KYWI, "col_idx": 0, "row_idx": 0 },
            { "slot_id": "belt_outer_pals", "component_sku": KYWI, "col_idx": 1, "row_idx": 0 },
        ],
    });
    let (status, body) = post(&app, "/api/v1/loadouts", colliding).await;
    assert_eq!(status, StatusCode::OK, "semantic errors are never HTTP errors: {body:?}");
    assert_eq!(body["persisted"], false);
    assert_eq!(body["valid"], false);
    let errors = body["summary"]["errors"].as_array().expect("structured errors");
    assert!(!errors.is_empty());
    assert!(errors[0].as_str().unwrap().contains("PALS"), "errors: {errors:?}");
    // Only the first pouch made it into the summary BOM.
    assert_eq!(body["summary"]["bom"].as_array().unwrap().len(), 1);
    assert!(body.get("id").is_none(), "nothing was persisted, so no id exists");

    // The store stayed empty.
    let (status, body) = get(&app, "/api/v1/loadouts").await;
    assert_eq!(status, StatusCode::OK);
    assert!(body["loadouts"].as_array().unwrap().is_empty());

    // A PATCH with invalid placements is likewise a 200 that writes nothing:
    // the row keeps its version and placements.
    let id = create_valid(&app).await;
    let (status, body) = patch(
        &app,
        &format!("/api/v1/loadouts/{id}"),
        serde_json::json!({
            "expected_version": 1,
            "placements": [
                { "slot_id": "belt_outer_pals", "component_sku": KYWI, "col_idx": 0, "row_idx": 0 },
                { "slot_id": "belt_outer_pals", "component_sku": KYWI, "col_idx": 1, "row_idx": 0 },
            ],
        }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "body: {body:?}");
    assert_eq!(body["persisted"], false);
    assert_eq!(body["valid"], false);
    assert!(!body["summary"]["errors"].as_array().unwrap().is_empty());

    let (status, detail) = get(&app, &format!("/api/v1/loadouts/{id}")).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(detail["version"], 1, "row untouched by the rejected PATCH");
    assert_eq!(detail["placements"].as_array().unwrap().len(), 2);
    assert_eq!(detail["platform_sku"], JPC, "platform unchanged too");
}

#[tokio::test]
async fn transport_errors_use_status_codes() {
    let (app, _store, _dir) = persisted_app().await;

    // Unknown loadout id: 404 across the persisted routes.
    let ghost = uuid::Uuid::new_v4().to_string();
    let (status, _) = get(&app, &format!("/api/v1/loadouts/{ghost}")).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    let (status, _) =
        patch(&app, &format!("/api/v1/loadouts/{ghost}"), serde_json::json!({ "expected_version": 1 })).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    let (status, _) = delete(&app, &format!("/api/v1/loadouts/{ghost}")).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    let (status, _) = get(&app, &format!("/api/v1/loadouts/{ghost}/evaluate")).await;
    assert_eq!(status, StatusCode::NOT_FOUND);

    // Malformed ids (not UUIDs) are 400s from the path extractor.
    let (status, _) = get(&app, "/api/v1/loadouts/not-a-uuid").await;
    assert_eq!(status, StatusCode::BAD_REQUEST);

    // Unknown platform on create: 404, same as /evaluate.
    let (status, body) = post(
        &app,
        "/api/v1/loadouts",
        serde_json::json!({ "platform_sku": "no-such-carrier", "placements": [] }),
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    assert!(body["error"].as_str().unwrap().contains("no-such-carrier"));

    // A missing store answers 503 rather than pretending to persist.
    let catalog = load_catalog(None).expect("catalog");
    let stateless = build_router(AppState::new(catalog));
    let (status, body) = post(&stateless, "/api/v1/loadouts", valid_build()).await;
    assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE, "body: {body:?}");
    let (status, _) = get(&stateless, "/api/v1/loadouts").await;
    assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE);
}

// --- evaluate route ------------------------------------------------------------------

#[tokio::test]
async fn evaluate_route_supports_pinned_and_cross_revision_evaluation() {
    let (app, store, _dir) = persisted_app().await;
    let id = create_valid(&app).await;

    // Pinned revision (default and explicit): a valid, fail-closed replay.
    let (status, body) = get(&app, &format!("/api/v1/loadouts/{id}/evaluate")).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["id"], id);
    assert_eq!(body["catalog_version"], 1);
    assert_eq!(body["summary"]["valid"], true);
    let (status, body) = get(&app, &format!("/api/v1/loadouts/{id}/evaluate?catalog_version=1")).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["catalog_version"], 1);
    assert_eq!(event_types(&store, &id).await, vec!["created".to_owned()],
        "pinned evaluations are plain reads, not audited actions");

    // Unknown revisions and malformed parameters are transport errors.
    let (status, _) = get(&app, &format!("/api/v1/loadouts/{id}/evaluate?catalog_version=999")).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    let (status, _) = get(&app, &format!("/api/v1/loadouts/{id}/evaluate?catalog_version=abc")).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);

    // Publish revision 2 (same catalog body) and evaluate across it: the
    // cross-revision evaluation is a semantic result and is audited.
    sqlx::query(
        "INSERT INTO catalog_revisions (version, seed_sha256, catalog_json, published_at)
         SELECT 2, seed_sha256, catalog_json, published_at FROM catalog_revisions WHERE version = 1",
    )
    .execute(store.pool())
    .await
    .expect("publish revision 2");
    let (status, body) = get(&app, &format!("/api/v1/loadouts/{id}/evaluate?catalog_version=2")).await;
    assert_eq!(status, StatusCode::OK, "body: {body:?}");
    assert_eq!(body["catalog_version"], 2);
    assert_eq!(body["summary"]["valid"], true);
    assert_eq!(
        event_types(&store, &id).await,
        vec!["created".to_owned(), "re_evaluated".to_owned()],
        "cross-revision evaluation is audited"
    );

    // New loadouts pin to the newly active revision.
    let (status, body) = post(&app, "/api/v1/loadouts", valid_build()).await;
    assert_eq!(status, StatusCode::CREATED);
    assert_eq!(body["catalog_version"], 2, "creation pins the active revision");
}

#[tokio::test]
async fn corrupt_rows_fail_closed_through_the_api() {
    let (app, store, _dir) = persisted_app().await;
    let id = create_valid(&app).await;

    // Corrupt the stored placements out-of-band; reads must fail closed
    // with the structured R2 error, never a coerced summary.
    sqlx::query("UPDATE loadouts SET placements_json = '{not json' WHERE id = ?")
        .bind(&id)
        .execute(store.pool())
        .await
        .expect("corrupt row");

    let (status, body) = get(&app, &format!("/api/v1/loadouts/{id}")).await;
    assert_eq!(status, StatusCode::INTERNAL_SERVER_ERROR, "body: {body:?}");
    assert_eq!(body["error"], "corrupt_or_invalid_record");
    assert!(body["detail"].as_str().unwrap().contains("placements_json"));

    let (status, body) = get(&app, &format!("/api/v1/loadouts/{id}/evaluate")).await;
    assert_eq!(status, StatusCode::INTERNAL_SERVER_ERROR);
    assert_eq!(body["error"], "corrupt_or_invalid_record");

    // Even the list fails closed rather than skipping the bad row.
    let (status, _) = get(&app, "/api/v1/loadouts").await;
    assert_eq!(status, StatusCode::INTERNAL_SERVER_ERROR);
}

// --- regression: stateless routes unchanged ------------------------------------------

#[tokio::test]
async fn stateless_routes_still_work_alongside_persistence() {
    let (app, _store, _dir) = persisted_app().await;

    // The pre-existing evaluate endpoint behaves exactly as before.
    let (status, body) = post(&app, "/api/v1/loadouts/evaluate", valid_build()).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["valid"], true);
    assert!(body.get("persisted").is_none(), "stateless route has no persistence fields");
    let (status, body) = get(&app, "/api/v1/catalog/platforms").await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body.as_array().unwrap().len(), 29);
    let (status, body) = post(&app, "/api/v1/loadouts/from-preset/jtac", serde_json::json!({})).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["valid"], true);

    // Nothing above persisted anything.
    let (status, body) = get(&app, "/api/v1/loadouts").await;
    assert_eq!(status, StatusCode::OK);
    assert!(body["loadouts"].as_array().unwrap().is_empty());
}
