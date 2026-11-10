//! `store` — the SQLite persistence layer (Spec 05).
//!
//! Durable storage for catalog revisions, vendor offers, role presets, and
//! server-side loadout documents, built on `sqlx` over SQLite.
//!
//! Design invariants:
//!
//! - **R2 (fail-closed)**: every read deserializes through `core-engine`
//!   types, so a corrupted or invalid row surfaces as
//!   [`StoreError::CorruptOrInvalidRecord`] instead of being silently
//!   coerced. Catalog revisions are loaded from the canonical
//!   `catalog_json` document (serde enforces the enum wire strings) with
//!   the *current* `vendor_offers` rows overlaid, so pricing always reflects
//!   live offers while structure stays pinned to the published revision.
//! - **R3 (typed enums down to storage)**: the DDL CHECK constraints
//!   mirror the engine's enum wire strings (see
//!   `migrations/0001_initial_schema.sql`), so invalid values cannot be
//!   written even through raw SQL.
//! - **Optimistic locking**: `loadouts.version` is monotonic from 1;
//!   [`Store::update_loadout`] only mutates the row when the caller's
//!   `expected_version` matches, returning
//!   [`StoreError::VersionConflict`] otherwise.
//! - **Audit trail**: every create/update appends a row to the append-only
//!   `loadout_events` table inside the same transaction. `deleted` events
//!   cascade away with their loadout (the DDL ties events to the row), so
//!   the surviving trail documents the build's lifetime between create and
//!   delete.
//!
//! The engine itself stays pure: this crate *consumes* `core-engine` types
//! and never mutates them (R1).

#![forbid(unsafe_code)]
#![warn(missing_docs)]

use std::collections::HashMap;
use std::str::FromStr;
use std::time::Duration;

use core_engine::catalog::Catalog;
use core_engine::pricing::{Colorway, VendorOffer};
use core_engine::regulation::{OperationalRole, RegulationPolicy};
use serde::{Deserialize, Serialize};
use serde::de::DeserializeOwned;
use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions, SqliteRow};
use sqlx::{Executor, Row, Sqlite, SqlitePool};
use thiserror::Error;
use uuid::Uuid;

/// The loadout document schema version this crate writes and understands.
pub const STORE_SCHEMA_VERSION: i64 = 1;

/// Every failure mode of the persistence layer.
#[derive(Debug, Error)]
pub enum StoreError {
    /// A database-level failure (connection, constraint, migration).
    #[error("database error: {0}")]
    Database(String),
    /// A stored record failed `core-engine` validation on read (R2) — the
    /// row was never silently coerced or defaulted.
    #[error("stored record is corrupt or failed core-engine validation: {detail}")]
    CorruptOrInvalidRecord {
        /// What was read and why it failed.
        detail: String,
    },
    /// No loadout with the requested id exists.
    #[error("unknown loadout id `{0}`")]
    UnknownLoadout(Uuid),
    /// The caller's expected version does not match the stored version.
    #[error("version conflict: current version is {current_version}")]
    VersionConflict {
        /// The version actually stored in the row.
        current_version: i64,
    },
    /// No catalog revision with the requested version exists.
    #[error("unknown catalog revision {0}")]
    UnknownCatalogRevision(i64),
}

/// Wrap a `sqlx` error as a [`StoreError::Database`].
fn db(error: sqlx::Error) -> StoreError {
    StoreError::Database(error.to_string())
}

/// Build a [`StoreError::CorruptOrInvalidRecord`] with a detail message.
fn corrupt(detail: impl Into<String>) -> StoreError {
    StoreError::CorruptOrInvalidRecord {
        detail: detail.into(),
    }
}

/// Serialize an enum to its wire string (the same string the DDL CHECKs
/// and serde use).
fn enum_wire<T: Serialize>(field: &str, value: T) -> Result<String, StoreError> {
    serde_json::to_value(&value)
        .ok()
        .and_then(|value| value.as_str().map(str::to_owned))
        .ok_or_else(|| corrupt(format!("`{field}` did not serialize to a wire string")))
}

/// Parse an enum from its stored wire string (R2: parse failures are
/// corruption, never defaults).
fn parse_enum<T: DeserializeOwned>(field: &str, raw: &str) -> Result<T, StoreError> {
    serde_json::from_value(serde_json::Value::String(raw.to_owned()))
        .map_err(|error| corrupt(format!("`{field}` value `{raw}` is not a valid enum: {error}")))
}

/// One persisted placement — the canonical `placements_json` element.
///
/// Field names match the API's `PlacementRequest` shape so the same JSON
/// documents flow between client, API, and storage unchanged.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StoredPlacement {
    /// Target slot: a platform slot id or `<mount_id>.<child_slot_id>` path.
    pub slot_id: String,
    /// SKU of the equipped component.
    pub component_sku: String,
    /// PALS column index for grid placements.
    pub col_idx: u8,
    /// PALS row index for grid placements.
    pub row_idx: u8,
}

/// List-view metadata for a persisted loadout.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LoadoutMetadata {
    /// Loadout id.
    pub id: Uuid,
    /// Base platform SKU.
    pub platform_sku: String,
    /// The catalog revision this loadout is pinned to.
    pub catalog_version: i64,
    /// Monotonic document version (optimistic lock).
    pub version: i64,
    /// Configured colorway.
    pub colorway: Colorway,
    /// Regulation policy, when one was chosen.
    pub regulation_policy: Option<RegulationPolicy>,
    /// Operational role, when one was chosen.
    pub operational_role: Option<OperationalRole>,
    /// RFC 3339 creation timestamp.
    pub created_at: String,
    /// RFC 3339 last-update timestamp.
    pub updated_at: String,
}

/// A full persisted loadout document.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PersistedLoadout {
    /// Row metadata (id, versions, timestamps, ...).
    #[serde(flatten)]
    pub metadata: LoadoutMetadata,
    /// The canonical placement list.
    pub placements: Vec<StoredPlacement>,
    /// Storage schema version of the row.
    pub schema_version: i64,
}

/// The SQLite-backed repository.
pub struct Store {
    pool: SqlitePool,
}

impl Store {
    /// Connect to `db_url` and run all embedded migrations.
    ///
    /// Accepts `sqlx` SQLite URLs (`sqlite://file.db?mode=rwc`,
    /// `sqlite::memory:`). Foreign keys are enforced and short lock waits
    /// are retried (5 s busy timeout). A `:memory:` URL pins the pool to a
    /// single connection — separate in-memory connections would otherwise
    /// see separate empty databases.
    pub async fn connect_and_migrate(db_url: &str) -> Result<Self, StoreError> {
        let options = SqliteConnectOptions::from_str(db_url)
            .map_err(db)?
            .foreign_keys(true)
            .busy_timeout(Duration::from_secs(5));
        let max_connections = if db_url.contains(":memory:") { 1 } else { 8 };
        let pool = SqlitePoolOptions::new()
            .max_connections(max_connections)
            .connect_with(options)
            .await
            .map_err(db)?;
        sqlx::migrate!("./migrations")
            .run(&pool)
            .await
            .map_err(|error| db(error.into()))?;
        Ok(Self { pool })
    }

    /// Direct SQL access, for tests and maintenance only. Application code
    /// goes through the repository methods above.
    pub fn pool(&self) -> &SqlitePool {
        &self.pool
    }

    /// The active (highest) catalog revision, or `0` when the database is
    /// still unseeded.
    pub async fn active_catalog_version(&self) -> Result<i64, StoreError> {
        let version: i64 = sqlx::query_scalar(
            "SELECT COALESCE(MAX(version), 0) FROM catalog_revisions",
        )
        .fetch_one(&self.pool)
        .await
        .map_err(db)?;
        Ok(version)
    }

    /// Bootstrap revision `1` from `seed_catalog` when the database is
    /// empty (single transaction), then validate the round-tripped catalog
    /// before committing. A second call is a no-op that simply reports the
    /// active revision.
    ///
    /// Returns the active catalog version.
    pub async fn ensure_seeded(&self, seed_catalog: &Catalog) -> Result<i64, StoreError> {
        let active = self.active_catalog_version().await?;
        if active > 0 {
            return Ok(active);
        }

        let canonical_json = serde_json::to_string(seed_catalog)
            .map_err(|e| corrupt(format!("seed serialization: {e}")))?;
        let published_at = now_rfc3339();
        let mut tx = self.pool.begin().await.map_err(db)?;

        // Re-check inside the transaction; the revision PK additionally
        // guards against a racing seeder.
        let already: i64 =
            sqlx::query_scalar("SELECT COALESCE(MAX(version), 0) FROM catalog_revisions")
                .fetch_one(&mut *tx)
                .await
                .map_err(db)?;
        if already > 0 {
            tx.commit().await.map_err(db)?;
            return Ok(already);
        }

        sqlx::query(
            "INSERT INTO catalog_revisions (version, seed_sha256, catalog_json, published_at) VALUES (1, ?, ?, ?)",
        )
        .bind(sha256_hex(canonical_json.as_bytes()))
        .bind(&canonical_json)
        .bind(&published_at)
        .execute(&mut *tx)
        .await
        .map_err(db)?;

        seed_projection_rows(&mut tx, seed_catalog).await?;

        // R2: the round-tripped revision must equal the seed exactly —
        // never trust a write that reads back differently.
        let round_tripped: String =
            sqlx::query_scalar("SELECT catalog_json FROM catalog_revisions WHERE version = 1")
                .fetch_one(&mut *tx)
                .await
                .map_err(db)?;
        let parsed: Catalog = serde_json::from_str(&round_tripped)
            .map_err(|e| corrupt(format!("seed round-trip parse: {e}")))?;
        if parsed != *seed_catalog {
            return Err(corrupt("seed round-trip mismatch: stored revision differs from seed"));
        }

        tx.commit().await.map_err(db)?;
        Ok(1)
    }

    /// Load catalog revision `version`, deserialized through `core-engine`
    /// (fail-closed, R2) with the *current* `vendor_offers` rows overlaid
    /// onto platforms, components, and adapters.
    pub async fn load_catalog_revision(&self, version: i64) -> Result<Catalog, StoreError> {
        let json: String = sqlx::query_scalar("SELECT catalog_json FROM catalog_revisions WHERE version = ?")
            .bind(version)
            .fetch_optional(&self.pool)
            .await
            .map_err(db)?
            .ok_or(StoreError::UnknownCatalogRevision(version))?;
        let mut catalog: Catalog = serde_json::from_str(&json)
            .map_err(|e| corrupt(format!("catalog revision {version}: {e}")))?;

        let offers = sqlx::query(
            "SELECT item_sku, vendor_name, price_cents, in_stock, product_url
             FROM vendor_offers WHERE catalog_version = ? ORDER BY id",
        )
        .bind(version)
        .fetch_all(&self.pool)
        .await
        .map_err(db)?;
        let mut by_sku: HashMap<String, Vec<VendorOffer>> = HashMap::new();
        for row in offers {
            let sku: String = row.try_get("item_sku").map_err(db)?;
            by_sku.entry(sku).or_default().push(VendorOffer {
                vendor_name: row.try_get("vendor_name").map_err(db)?,
                price_cents: u32::try_from(row.try_get::<i64, _>("price_cents").map_err(db)?)
                    .map_err(|_| corrupt("vendor offer price_cents out of range"))?,
                in_stock: row.try_get("in_stock").map_err(db)?,
                product_url: row.try_get("product_url").map_err(db)?,
            });
        }

        // The offer rows are the source of truth for *current* pricing;
        // structure stays pinned to the published revision.
        for item in &mut catalog.platforms {
            if !item.sku.is_empty() {
                item.vendor_offers = by_sku.get(&item.sku).cloned().unwrap_or_default();
            }
        }
        for item in &mut catalog.components {
            if !item.sku.is_empty() {
                item.vendor_offers = by_sku.get(&item.sku).cloned().unwrap_or_default();
            }
        }
        for item in &mut catalog.adapter_bridges {
            if !item.sku.is_empty() {
                item.vendor_offers = by_sku.get(&item.sku).cloned().unwrap_or_default();
            }
        }
        Ok(catalog)
    }

    /// Create a loadout document at `version` 1, pinned to
    /// `catalog_version`, and append the `created` audit event. The caller
    /// is responsible for having validated the placements through
    /// `core-engine` first (the API layer does).
    #[allow(clippy::too_many_arguments)]
    pub async fn create_loadout(
        &self,
        platform_sku: &str,
        catalog_version: i64,
        colorway: Colorway,
        regulation_policy: Option<RegulationPolicy>,
        operational_role: Option<OperationalRole>,
        placements: &[StoredPlacement],
    ) -> Result<PersistedLoadout, StoreError> {
        let id = Uuid::new_v4();
        let now = now_rfc3339();
        let placements_json = canon_placements(placements)?;
        let payload = state_event_payload(colorway, regulation_policy, operational_role, placements);

        let mut tx = self.pool.begin().await.map_err(db)?;
        sqlx::query(
            "INSERT INTO loadouts (
                id, platform_sku, catalog_version, version, colorway,
                regulation_policy, operational_role, placements_json,
                schema_version, created_at, updated_at
            ) VALUES (?, ?, ?, 1, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(id.to_string())
        .bind(platform_sku)
        .bind(catalog_version)
        .bind(enum_wire("colorway", colorway)?)
        .bind(regulation_policy.map(|p| enum_wire("regulation_policy", p)).transpose()?)
        .bind(operational_role.map(|r| enum_wire("operational_role", r)).transpose()?)
        .bind(&placements_json)
        .bind(STORE_SCHEMA_VERSION)
        .bind(&now)
        .bind(&now)
        .execute(&mut *tx)
        .await
        .map_err(db)?;
        insert_event(&mut *tx, &id, 1, "created", &payload).await?;
        tx.commit().await.map_err(db)?;

        self.get_loadout(id)
            .await?
            .ok_or_else(|| corrupt("loadout row vanished immediately after insert"))
    }

    /// Fetch one loadout document. `Ok(None)` means the id is unknown;
    /// a row that fails validation on read is
    /// [`StoreError::CorruptOrInvalidRecord`] (R2).
    pub async fn get_loadout(&self, id: Uuid) -> Result<Option<PersistedLoadout>, StoreError> {
        let row = sqlx::query(
            "SELECT id, platform_sku, catalog_version, version, colorway,
                    regulation_policy, operational_role, placements_json,
                    schema_version, created_at, updated_at
             FROM loadouts WHERE id = ?",
        )
        .bind(id.to_string())
        .fetch_optional(&self.pool)
        .await
        .map_err(db)?;
        match row {
            Some(row) => Ok(Some(loadout_from_row(&row)?)),
            None => Ok(None),
        }
    }

    /// List loadout metadata, newest update first.
    pub async fn list_loadouts(
        &self,
        limit: u32,
        offset: u32,
    ) -> Result<Vec<LoadoutMetadata>, StoreError> {
        let rows = sqlx::query(
            "SELECT id, platform_sku, catalog_version, version, colorway,
                    regulation_policy, operational_role, placements_json,
                    schema_version, created_at, updated_at
             FROM loadouts ORDER BY updated_at DESC, id ASC LIMIT ? OFFSET ?",
        )
        .bind(i64::from(limit))
        .bind(i64::from(offset))
        .fetch_all(&self.pool)
        .await
        .map_err(db)?;
        // Full-row parse: a corrupted placements_json fails the whole list
        // rather than being skipped (R2, fail-closed).
        rows.iter()
            .map(loadout_from_row)
            .map(|loadout| loadout.map(|loadout| loadout.metadata))
            .collect()
    }

    /// Update a loadout under optimistic locking: the row is only mutated
    /// when `expected_version` matches, `version` increments, and an
    /// `updated` audit event is appended — all in one transaction.
    ///
    /// A stale `expected_version` returns [`StoreError::VersionConflict`]
    /// with the stored version; an unknown id returns
    /// [`StoreError::UnknownLoadout`].
    #[allow(clippy::too_many_arguments)]
    pub async fn update_loadout(
        &self,
        id: Uuid,
        expected_version: i64,
        colorway: Colorway,
        regulation_policy: Option<RegulationPolicy>,
        operational_role: Option<OperationalRole>,
        placements: &[StoredPlacement],
    ) -> Result<PersistedLoadout, StoreError> {
        let now = now_rfc3339();
        let placements_json = canon_placements(placements)?;
        let payload = state_event_payload(colorway, regulation_policy, operational_role, placements);

        let mut tx = self.pool.begin().await.map_err(db)?;
        let result = sqlx::query(
            "UPDATE loadouts SET colorway = ?, regulation_policy = ?, operational_role = ?,
                    placements_json = ?, version = version + 1, updated_at = ?
             WHERE id = ? AND version = ?",
        )
        .bind(enum_wire("colorway", colorway)?)
        .bind(regulation_policy.map(|p| enum_wire("regulation_policy", p)).transpose()?)
        .bind(operational_role.map(|r| enum_wire("operational_role", r)).transpose()?)
        .bind(&placements_json)
        .bind(&now)
        .bind(id.to_string())
        .bind(expected_version)
        .execute(&mut *tx)
        .await
        .map_err(db)?;

        if result.rows_affected() == 0 {
            let current: Option<i64> =
                sqlx::query_scalar("SELECT version FROM loadouts WHERE id = ?")
                    .bind(id.to_string())
                    .fetch_optional(&mut *tx)
                    .await
                    .map_err(db)?;
            return match current {
                Some(current_version) => Err(StoreError::VersionConflict { current_version }),
                None => Err(StoreError::UnknownLoadout(id)),
            };
        }

        let row = sqlx::query(
            "SELECT id, platform_sku, catalog_version, version, colorway,
                    regulation_policy, operational_role, placements_json,
                    schema_version, created_at, updated_at
             FROM loadouts WHERE id = ?",
        )
        .bind(id.to_string())
        .fetch_one(&mut *tx)
        .await
        .map_err(db)?;
        let updated = loadout_from_row(&row)?;
        insert_event(&mut *tx, &id, updated.metadata.version, "updated", &payload).await?;
        tx.commit().await.map_err(db)?;
        Ok(updated)
    }

    /// Delete a loadout; its audit events cascade away with the row.
    /// Returns whether a row was deleted.
    pub async fn delete_loadout(&self, id: Uuid) -> Result<bool, StoreError> {
        let result = sqlx::query("DELETE FROM loadouts WHERE id = ?")
            .bind(id.to_string())
            .execute(&self.pool)
            .await
            .map_err(db)?;
        Ok(result.rows_affected() > 0)
    }

    /// Append a `re_evaluated` audit event, recording that the loadout was
    /// explicitly evaluated against `evaluated_catalog_version`.
    pub async fn record_re_evaluation(
        &self,
        id: Uuid,
        evaluated_catalog_version: i64,
    ) -> Result<(), StoreError> {
        let version: i64 = sqlx::query_scalar("SELECT version FROM loadouts WHERE id = ?")
            .bind(id.to_string())
            .fetch_optional(&self.pool)
            .await
            .map_err(db)?
            .ok_or(StoreError::UnknownLoadout(id))?;
        let payload = serde_json::json!({ "evaluated_catalog_version": evaluated_catalog_version });
        let mut tx = self.pool.begin().await.map_err(db)?;
        insert_event(&mut *tx, &id, version, "re_evaluated", &payload).await?;
        tx.commit().await.map_err(db)?;
        Ok(())
    }
}

// --- row parsing -------------------------------------------------------------------

/// Parse one `loadouts` row into a [`PersistedLoadout`], fail-closed (R2).
fn loadout_from_row(row: &SqliteRow) -> Result<PersistedLoadout, StoreError> {
    let id_raw: String = row.try_get("id").map_err(db)?;
    let id = Uuid::parse_str(&id_raw).map_err(|_| corrupt(format!("loadout id `{id_raw}` is not a UUID")))?;
    let colorway_raw: String = row.try_get("colorway").map_err(db)?;
    let schema_version: i64 = row.try_get("schema_version").map_err(db)?;
    if schema_version != STORE_SCHEMA_VERSION {
        return Err(corrupt(format!(
            "loadout {id} has unsupported schema_version {schema_version} (expected {STORE_SCHEMA_VERSION})"
        )));
    }
    let placements_json: String = row.try_get("placements_json").map_err(db)?;
    let placements: Vec<StoredPlacement> = serde_json::from_str(&placements_json).map_err(|e| {
        corrupt(format!("loadout {id} has corrupt placements_json: {e}"))
    })?;

    Ok(PersistedLoadout {
        metadata: LoadoutMetadata {
            id,
            platform_sku: row.try_get("platform_sku").map_err(db)?,
            catalog_version: row.try_get("catalog_version").map_err(db)?,
            version: row.try_get("version").map_err(db)?,
            colorway: parse_enum("colorway", &colorway_raw)?,
            regulation_policy: parse_optional_enum(
                "regulation_policy",
                row.try_get("regulation_policy").map_err(db)?,
            )?,
            operational_role: parse_optional_enum(
                "operational_role",
                row.try_get("operational_role").map_err(db)?,
            )?,
            created_at: row.try_get("created_at").map_err(db)?,
            updated_at: row.try_get("updated_at").map_err(db)?,
        },
        placements,
        schema_version,
    })
}

/// Parse a nullable enum column.
fn parse_optional_enum<T: DeserializeOwned>(
    field: &str,
    raw: Option<String>,
) -> Result<Option<T>, StoreError> {
    match raw {
        None => Ok(None),
        Some(raw) => parse_enum(field, &raw).map(Some),
    }
}

// --- seeding helpers -----------------------------------------------------------------

/// Insert the relational projection rows (platforms, junctions, components,
/// adapters, vendor offers, presets) for revision 1 inside `tx`.
async fn seed_projection_rows(
    tx: &mut sqlx::Transaction<'_, Sqlite>,
    seed_catalog: &Catalog,
) -> Result<(), StoreError> {
    for platform in &seed_catalog.platforms {
        sqlx::query(
            "INSERT INTO platforms (catalog_version, sku, category, msrp_cents, spec_json)
             VALUES (1, ?, ?, ?, ?)",
        )
        .bind(&platform.sku)
        .bind(platform.category.map(|c| enum_wire("category", c)).transpose()?)
        .bind(i64::from(platform.msrp_cents))
        .bind(serde_json::to_string(platform).map_err(|e| corrupt(format!("platform serialization: {e}")))?)
        .execute(&mut **tx)
        .await
        .map_err(db)?;
        for cut in &platform.plate_cuts {
            sqlx::query(
                "INSERT INTO platform_plate_cuts (catalog_version, platform_sku, plate_cut) VALUES (1, ?, ?)",
            )
            .bind(&platform.sku)
            .bind(enum_wire("plate_cut", *cut)?)
            .execute(&mut **tx)
            .await
            .map_err(db)?;
        }
        for size in &platform.sizes {
            sqlx::query(
                "INSERT INTO platform_sizes (catalog_version, platform_sku, size) VALUES (1, ?, ?)",
            )
            .bind(&platform.sku)
            .bind(enum_wire("size", *size)?)
            .execute(&mut **tx)
            .await
            .map_err(db)?;
        }
        insert_vendor_offers(tx, &platform.sku, &platform.vendor_offers).await?;
    }

    for component in &seed_catalog.components {
        sqlx::query(
            "INSERT INTO components (catalog_version, sku, kind, msrp_cents, spec_json)
             VALUES (1, ?, ?, ?, ?)",
        )
        .bind(&component.sku)
        .bind(enum_wire("kind", component.kind)?)
        .bind(i64::from(component.msrp_cents))
        .bind(serde_json::to_string(component).map_err(|e| corrupt(format!("component serialization: {e}")))?)
        .execute(&mut **tx)
        .await
        .map_err(db)?;
        insert_vendor_offers(tx, &component.sku, &component.vendor_offers).await?;
    }

    for adapter in &seed_catalog.adapter_bridges {
        sqlx::query(
            "INSERT INTO adapter_bridges (catalog_version, sku, from_interface, to_interface, msrp_cents, spec_json)
             VALUES (1, ?, ?, ?, ?, ?)",
        )
        .bind(&adapter.sku)
        .bind(enum_wire("from_interface", adapter.platform_interface)?)
        .bind(enum_wire("to_interface", adapter.component_interface)?)
        .bind(i64::from(adapter.msrp_cents))
        .bind(serde_json::to_string(adapter).map_err(|e| corrupt(format!("adapter serialization: {e}")))?)
        .execute(&mut **tx)
        .await
        .map_err(db)?;
        insert_vendor_offers(tx, &adapter.sku, &adapter.vendor_offers).await?;
    }

    for preset in &seed_catalog.role_presets {
        sqlx::query(
            "INSERT INTO role_presets (catalog_version, preset_id, role, policy, platform_sku, preset_json)
             VALUES (1, ?, ?, ?, ?, ?)",
        )
        .bind(&preset.preset_id)
        .bind(preset.role.map(|r| enum_wire("role", r)).transpose()?)
        .bind(enum_wire("policy", preset.policy)?)
        .bind(&preset.platform_sku)
        .bind(serde_json::to_string(preset).map_err(|e| corrupt(format!("preset serialization: {e}")))?)
        .execute(&mut **tx)
        .await
        .map_err(db)?;
    }
    Ok(())
}

/// Insert the vendor offers of one item (skipped for empty SKUs, which
/// cannot be keyed).
async fn insert_vendor_offers(
    tx: &mut sqlx::Transaction<'_, Sqlite>,
    item_sku: &str,
    offers: &[VendorOffer],
) -> Result<(), StoreError> {
    if item_sku.is_empty() {
        return Ok(());
    }
    for offer in offers {
        sqlx::query(
            "INSERT INTO vendor_offers (catalog_version, item_sku, vendor_name, price_cents, in_stock, product_url)
             VALUES (1, ?, ?, ?, ?, ?)",
        )
        .bind(item_sku)
        .bind(&offer.vendor_name)
        .bind(i64::from(offer.price_cents))
        .bind(offer.in_stock)
        .bind(&offer.product_url)
        .execute(&mut **tx)
        .await
        .map_err(db)?;
    }
    Ok(())
}

// --- event helpers --------------------------------------------------------------------

/// Append one audit event row inside `tx`.
async fn insert_event<'e, E>(
    executor: E,
    loadout_id: &Uuid,
    version: i64,
    event_type: &str,
    payload: &serde_json::Value,
) -> Result<(), StoreError>
where
    E: Executor<'e, Database = Sqlite>,
{
    sqlx::query(
        "INSERT INTO loadout_events (loadout_id, version, event_type, payload_json, occurred_at)
         VALUES (?, ?, ?, ?, ?)",
    )
    .bind(loadout_id.to_string())
    .bind(version)
    .bind(event_type)
    .bind(serde_json::to_string(payload).map_err(|e| corrupt(format!("event payload: {e}")))?)
    .bind(now_rfc3339())
    .execute(executor)
    .await
    .map_err(db)?;
    Ok(())
}

/// The audit payload snapshot for `created`/`updated` events.
fn state_event_payload(
    colorway: Colorway,
    regulation_policy: Option<RegulationPolicy>,
    operational_role: Option<OperationalRole>,
    placements: &[StoredPlacement],
) -> serde_json::Value {
    serde_json::json!({
        "colorway": colorway,
        "regulation_policy": regulation_policy,
        "operational_role": operational_role,
        "placement_count": placements.len(),
    })
}

// --- small utilities -------------------------------------------------------------------

/// Canonical serialization of a placement list.
fn canon_placements(placements: &[StoredPlacement]) -> Result<String, StoreError> {
    serde_json::to_string(placements).map_err(|e| corrupt(format!("placements serialization: {e}")))
}

/// Current UTC time, RFC 3339 formatted.
fn now_rfc3339() -> String {
    chrono::Utc::now().to_rfc3339()
}

/// Lowercase hex SHA-256 of `bytes`.
fn sha256_hex(bytes: &[u8]) -> String {
    use sha2::{Digest, Sha256};
    Sha256::digest(bytes)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

// keep the DeserializeOwned import co-located with the serde imports above
