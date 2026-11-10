//! Spec-05 store integration tests: bootstrap & idempotency, DDL CHECK
//! enforcement, R2 fail-closed corruption handling, loadout CRUD with the
//! audit trail, and optimistic-lock version conflicts.
//!
//! Every test boots against its own isolated temporary SQLite file with
//! migrations applied, seeded from the committed `catalog_seed.json` (the
//! in-memory `data-ingest` synthesis is the fallback when the seed file is
//! absent).

use std::path::Path;

use core_engine::catalog::Catalog;
use core_engine::pricing::Colorway;
use store::{PersistedLoadout, Store, StoreError, StoredPlacement};

const SEED_PATH: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../seeds/catalog_seed.json"
);

const JPC: &str = "crye-precision-jpc-2-0";
const MK5: &str = "spiritus-mk5-placard";
const KYWI: &str = "esstac-kywi-556-double";

// --- helpers -------------------------------------------------------------------

/// Load the seed catalog: the committed JSON when present, otherwise the
/// data-ingest synthesis (same shape the API falls back to).
fn seed_catalog() -> Catalog {
    if Path::new(SEED_PATH).is_file() {
        let raw = std::fs::read_to_string(SEED_PATH).expect("read seed json");
        serde_json::from_str(&raw).expect("parse seed json")
    } else {
        let dir = tempfile::tempdir().expect("temp dir");
        data_ingest::sample_csvs::write_sample_csvs(dir.path()).expect("sample CSVs");
        data_ingest::normalizer::normalize_catalog(dir.path())
            .expect("normalize catalog")
            .catalog
    }
}

/// A fresh migrated store over an isolated temporary SQLite file. The
/// returned `TempDir` keeps the file alive for the store's lifetime.
async fn fresh_store() -> (tempfile::TempDir, Store) {
    let dir = tempfile::tempdir().expect("temp dir");
    let url = format!("sqlite://{}?mode=rwc", dir.path().join("test.db").display());
    let store = Store::connect_and_migrate(&url).await.expect("connect + migrate");
    let seeded = store.ensure_seeded(&seed_catalog()).await.expect("seed");
    assert_eq!(seeded, 1, "fresh database seeds revision 1");
    (dir, store)
}

/// Count rows matching `sql`.
async fn count(store: &Store, sql: &str) -> i64 {
    let (count,): (i64,) = sqlx::query_as(sql).fetch_one(store.pool()).await.expect("count");
    count
}

/// A representative valid placement list (JPC 2.0 + MK5 placard + KYWI
/// pouch on the placard's front PALS field).
fn sample_placements() -> Vec<StoredPlacement> {
    vec![
        StoredPlacement {
            slot_id: "front_placard".to_owned(),
            component_sku: MK5.to_owned(),
            col_idx: 0,
            row_idx: 0,
        },
        StoredPlacement {
            slot_id: "m2.placard_front_pals".to_owned(),
            component_sku: KYWI.to_owned(),
            col_idx: 0,
            row_idx: 0,
        },
    ]
}

/// Create a loadout under test defaults.
async fn create_sample(store: &Store) -> PersistedLoadout {
    store
        .create_loadout(JPC, 1, Colorway::Multicam, None, None, &sample_placements())
        .await
        .expect("create loadout")
}

// --- gate 1: bootstrap & idempotency ----------------------------------------------

#[tokio::test]
async fn bootstrap_seeds_revision_1_with_the_full_catalog() {
    let (_dir, store) = fresh_store().await;

    let catalog = store.load_catalog_revision(1).await.expect("revision 1");
    assert_eq!(catalog.platforms.len(), 29);
    assert_eq!(catalog.components.len(), 12);
    assert_eq!(catalog.adapter_bridges.len(), 4);
    assert_eq!(catalog.role_presets.len(), 6);
    assert_eq!(catalog.platform(JPC).expect("JPC seeded").msrp_cents, 29_900);

    // The projection tables carry the full relational copy.
    assert_eq!(count(&store, "SELECT COUNT(*) FROM platforms").await, 29);
    assert_eq!(count(&store, "SELECT COUNT(*) FROM components").await, 12);
    assert_eq!(count(&store, "SELECT COUNT(*) FROM adapter_bridges").await, 4);
    assert_eq!(count(&store, "SELECT COUNT(*) FROM role_presets").await, 6);
    assert_eq!(count(&store, "SELECT COUNT(*) FROM vendor_offers").await, 100);
    assert_eq!(
        count(&store, "SELECT COUNT(*) FROM platform_plate_cuts").await,
        36
    );
    assert_eq!(count(&store, "SELECT COUNT(*) FROM platform_sizes").await, 116);

    // The revision row is pinned and self-describing.
    let (sha256, json): (String, String) = sqlx::query_as(
        "SELECT seed_sha256, catalog_json FROM catalog_revisions WHERE version = 1",
    )
    .fetch_one(store.pool())
    .await
    .expect("revision row");
    assert_eq!(sha256.len(), 64, "sha256 hex digest");
    assert!(json.contains(JPC));

    // Unknown revisions are a distinct, typed error.
    match store.load_catalog_revision(99).await {
        Err(StoreError::UnknownCatalogRevision(99)) => {}
        other => panic!("expected UnknownCatalogRevision(99), got {other:?}"),
    }
}

#[tokio::test]
async fn ensure_seeded_is_idempotent() {
    let (_dir, store) = fresh_store().await;

    let second = store.ensure_seeded(&seed_catalog()).await.expect("re-seed");
    assert_eq!(second, 1, "second call reports the same revision");
    assert_eq!(store.active_catalog_version().await.expect("active"), 1);
    assert_eq!(count(&store, "SELECT COUNT(*) FROM platforms").await, 29);
    assert_eq!(count(&store, "SELECT COUNT(*) FROM components").await, 12);
    assert_eq!(count(&store, "SELECT COUNT(*) FROM role_presets").await, 6);
    assert_eq!(
        count(&store, "SELECT COUNT(*) FROM catalog_revisions").await,
        1,
        "no duplicate revision rows"
    );
}

#[tokio::test]
async fn round_tripped_revision_equals_the_seed_catalog() {
    let (_dir, store) = fresh_store().await;
    let catalog = store.load_catalog_revision(1).await.expect("revision 1");
    assert_eq!(catalog, seed_catalog(), "offers overlay must reproduce the seed exactly");
}

// --- gate 2: CHECK constraints & R2 fail-closed -------------------------------------

#[tokio::test]
async fn check_constraints_reject_invalid_enum_strings() {
    let (_dir, store) = fresh_store().await;

    let bad_platform = sqlx::query(
        "INSERT INTO platforms (catalog_version, sku, category, msrp_cents, spec_json)
         VALUES (1, 'ghost-carrier', 'warship', 100, '{}')",
    )
    .execute(store.pool())
    .await;
    let error = bad_platform.expect_err("invalid platforms.category must be rejected");
    assert!(error.to_string().contains("CHECK"), "error: {error}");

    let bad_loadout = sqlx::query(
        "INSERT INTO loadouts (
            id, platform_sku, catalog_version, version, colorway,
            placements_json, schema_version, created_at, updated_at
        ) VALUES ('11111111-1111-1111-1111-111111111111', 'p', 1, 1, 'mauve', '[]', 1, 't', 't')",
    )
    .execute(store.pool())
    .await;
    let error = bad_loadout.expect_err("invalid loadouts.colorway must be rejected");
    assert!(error.to_string().contains("CHECK"), "error: {error}");

    let bad_cut = sqlx::query(
        "INSERT INTO platform_plate_cuts (catalog_version, platform_sku, plate_cut)
         VALUES (1, 'ghost-carrier', 'Rhoomba')",
    )
    .execute(store.pool())
    .await;
    let error = bad_cut.expect_err("invalid plate_cut must be rejected");
    assert!(error.to_string().contains("CHECK"), "error: {error}");

    // The failed inserts left nothing behind.
    assert_eq!(count(&store, "SELECT COUNT(*) FROM platforms WHERE sku = 'ghost-carrier'").await, 0);
    assert_eq!(count(&store, "SELECT COUNT(*) FROM loadouts").await, 0);
}

#[tokio::test]
async fn corrupt_rows_fail_closed_on_read() {
    let (_dir, store) = fresh_store().await;
    let created = create_sample(&store).await;

    // Corrupt placements_json directly.
    sqlx::query("UPDATE loadouts SET placements_json = '{not json' WHERE id = ?")
        .bind(created.metadata.id.to_string())
        .execute(store.pool())
        .await
        .expect("corrupt row");
    match store.get_loadout(created.metadata.id).await {
        Err(StoreError::CorruptOrInvalidRecord { detail }) => {
            assert!(detail.contains("placements_json"), "detail: {detail}");
        }
        other => panic!("expected CorruptOrInvalidRecord, got {other:?}"),
    }

    // Structurally valid JSON with the wrong shape fails closed too.
    sqlx::query("UPDATE loadouts SET placements_json = '[{\"slot_id\": 42}]' WHERE id = ?")
        .bind(created.metadata.id.to_string())
        .execute(store.pool())
        .await
        .expect("corrupt row");
    assert!(matches!(
        store.get_loadout(created.metadata.id).await,
        Err(StoreError::CorruptOrInvalidRecord { .. })
    ));

    // An unsupported schema_version is corruption, not a silent default.
    sqlx::query("UPDATE loadouts SET placements_json = '[]', schema_version = 7 WHERE id = ?")
        .bind(created.metadata.id.to_string())
        .execute(store.pool())
        .await
        .expect("bump schema version");
    assert!(matches!(
        store.get_loadout(created.metadata.id).await,
        Err(StoreError::CorruptOrInvalidRecord { .. })
    ));

    // A corrupt catalog revision document fails closed as well (R2).
    sqlx::query("UPDATE catalog_revisions SET catalog_json = '{\"platforms\": ' WHERE version = 1")
        .execute(store.pool())
        .await
        .expect("corrupt revision");
    assert!(matches!(
        store.load_catalog_revision(1).await,
        Err(StoreError::CorruptOrInvalidRecord { .. })
    ));
}

// --- gate 3: CRUD + audit trail ------------------------------------------------------

#[tokio::test]
async fn loadout_crud_and_audit_trail() {
    let (_dir, store) = fresh_store().await;
    let created = create_sample(&store).await;

    assert_eq!(created.metadata.version, 1);
    assert_eq!(created.metadata.catalog_version, 1);
    assert_eq!(created.metadata.platform_sku, JPC);
    assert_eq!(created.metadata.colorway, Colorway::Multicam);
    assert_eq!(created.placements, sample_placements());
    assert!(created.metadata.created_at.contains('T'), "RFC 3339 timestamp");

    // GET round-trips the exact document.
    let fetched = store
        .get_loadout(created.metadata.id)
        .await
        .expect("get loadout")
        .expect("row exists");
    assert_eq!(fetched, created);

    // Unknown ids are None, not an error.
    assert!(store
        .get_loadout(uuid::Uuid::new_v4())
        .await
        .expect("get unknown")
        .is_none());

    // PATCH: version 1 -> 2 with a colorway change.
    let updated = store
        .update_loadout(
            created.metadata.id,
            1,
            Colorway::CoyoteBrown,
            None,
            None,
            &sample_placements(),
        )
        .await
        .expect("update loadout");
    assert_eq!(updated.metadata.version, 2);
    assert_eq!(updated.metadata.colorway, Colorway::CoyoteBrown);
    assert_eq!(updated.metadata.created_at, created.metadata.created_at);

    // The audit trail carries created + updated events.
    let events: Vec<(String, i64, String)> = sqlx::query_as(
        "SELECT event_type, version, payload_json FROM loadout_events WHERE loadout_id = ? ORDER BY event_id",
    )
    .bind(created.metadata.id.to_string())
    .fetch_all(store.pool())
    .await
    .expect("events");
    assert_eq!(events.len(), 2);
    assert_eq!(events[0].0, "created");
    assert_eq!(events[0].1, 1);
    assert!(events[0].2.contains("\"placement_count\":2"), "payload: {}", events[0].2);
    assert_eq!(events[1].0, "updated");
    assert_eq!(events[1].1, 2);
    assert!(events[1].2.contains("\"colorway\":\"coyote_brown\""), "payload: {}", events[1].2);

    // Listing shows the metadata, newest update first.
    let listed = store.list_loadouts(10, 0).await.expect("list");
    assert_eq!(listed.len(), 1);
    assert_eq!(listed[0].id, created.metadata.id);
    assert_eq!(listed[0].version, 2);

    // DELETE removes the row and cascades the audit trail away.
    assert!(store.delete_loadout(created.metadata.id).await.expect("delete"));
    assert!(store.get_loadout(created.metadata.id).await.expect("get").is_none());
    assert_eq!(count(&store, "SELECT COUNT(*) FROM loadout_events").await, 0);
    assert!(!store.delete_loadout(created.metadata.id).await.expect("re-delete"));
}

#[tokio::test]
async fn re_evaluation_is_audited() {
    let (_dir, store) = fresh_store().await;
    let created = create_sample(&store).await;

    store
        .record_re_evaluation(created.metadata.id, 1)
        .await
        .expect("record evaluation");
    let (event_type, payload): (String, String) = sqlx::query_as(
        "SELECT event_type, payload_json FROM loadout_events WHERE loadout_id = ? AND event_type = 're_evaluated'",
    )
    .bind(created.metadata.id.to_string())
    .fetch_one(store.pool())
    .await
    .expect("re_evaluated event");
    assert_eq!(event_type, "re_evaluated");
    assert!(payload.contains("\"evaluated_catalog_version\":1"), "payload: {payload}");

    // Unknown loadout ids are a typed error.
    assert!(matches!(
        store.record_re_evaluation(uuid::Uuid::new_v4(), 1).await,
        Err(StoreError::UnknownLoadout(_))
    ));
}

// --- gate 4: optimistic concurrency ---------------------------------------------------

#[tokio::test]
async fn update_loadout_rejects_stale_expected_versions() {
    let (_dir, store) = fresh_store().await;
    let created = create_sample(&store).await;

    // First writer moves the row to version 2.
    let first = store
        .update_loadout(created.metadata.id, 1, Colorway::Multicam, None, None, &[])
        .await
        .expect("first update wins");
    assert_eq!(first.metadata.version, 2);

    // A second writer still presenting version 1 conflicts.
    match store
        .update_loadout(created.metadata.id, 1, Colorway::Multicam, None, None, &[])
        .await
    {
        Err(StoreError::VersionConflict { current_version }) => assert_eq!(current_version, 2),
        other => panic!("expected VersionConflict, got {other:?}"),
    }

    // The conflicting attempt wrote neither the row nor an event: the row
    // still holds the first writer's state (version 2, cleared placements).
    let current = store.get_loadout(created.metadata.id).await.expect("get").expect("row");
    assert_eq!(current.metadata.version, 2);
    assert!(current.placements.is_empty(), "first writer's placements stand");
    assert_eq!(count(&store, "SELECT COUNT(*) FROM loadout_events").await, 2);

    // Updating an unknown id is a 404-shaped error, not a version conflict.
    assert!(matches!(
        store
            .update_loadout(uuid::Uuid::new_v4(), 1, Colorway::Multicam, None, None, &[])
            .await,
        Err(StoreError::UnknownLoadout(_))
    ));
}

#[tokio::test]
async fn loadouts_reference_their_pinned_revision() {
    let (_dir, store) = fresh_store().await;

    // The FK rejects loadouts pinned to a nonexistent revision.
    let result = store
        .create_loadout(JPC, 99, Colorway::Multicam, None, None, &[])
        .await;
    assert!(
        matches!(result, Err(StoreError::Database(_))),
        "unknown catalog_version must be rejected by the schema"
    );
    assert_eq!(count(&store, "SELECT COUNT(*) FROM loadouts").await, 0);
}
