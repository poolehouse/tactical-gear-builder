-- 0001_initial_schema.sql — Spec 05 persistence layer.
--
-- STRICT tables enforce column types; CHECK constraints mirror the
-- `core-engine` enum wire strings exactly (R3):
--   PlatformCategory / Colorway / RegulationPolicy / OperationalRole
--     serialize snake_case ("plate_carrier", "multicam", "us_army_ar670_1").
--   ComponentKind / InterfaceStandard / PlateCut / Size serialize as bare
--     variant names ("Placard", "SwiftClip1InchQasm", "SapiEsapi", "Small").
-- The full CHECK lists cover every engine variant, not just the ones the
-- current seed exercises, so a future catalog never trips a stale DDL.
--
-- `platforms.plate_cut` / platform sizes are Vec-valued on PlatformSpec, so
-- they are projected into the `platform_plate_cuts` / `platform_sizes`
-- junction tables (one CHECKed row per accepted cut/size) — the canonical
-- document itself lives in `spec_json`.
--
-- All writes flow through `crates/store`; revision rows are append-only and
-- never mutated. Reads fail closed through `core-engine` serde (R2).

CREATE TABLE catalog_revisions (
    version INTEGER PRIMARY KEY CHECK (version >= 1),
    seed_sha256 TEXT NOT NULL,
    catalog_json TEXT NOT NULL,
    published_at TEXT NOT NULL
) STRICT;

CREATE TABLE platforms (
    catalog_version INTEGER NOT NULL REFERENCES catalog_revisions(version),
    sku TEXT NOT NULL,
    category TEXT CHECK (
        category IS NULL
        OR category IN ('plate_carrier', 'belt', 'chest_rig', 'lbe')
    ),
    msrp_cents INTEGER NOT NULL CHECK (msrp_cents >= 0),
    spec_json TEXT NOT NULL,
    PRIMARY KEY (catalog_version, sku)
) STRICT;

CREATE TABLE platform_plate_cuts (
    catalog_version INTEGER NOT NULL REFERENCES catalog_revisions(version),
    platform_sku TEXT NOT NULL,
    plate_cut TEXT NOT NULL CHECK (
        plate_cut IN ('SapiEsapi', 'Swimmer', 'Shooter')
    ),
    PRIMARY KEY (catalog_version, platform_sku, plate_cut)
) STRICT;

CREATE TABLE platform_sizes (
    catalog_version INTEGER NOT NULL REFERENCES catalog_revisions(version),
    platform_sku TEXT NOT NULL,
    size TEXT NOT NULL CHECK (
        size IN ('Small', 'Medium', 'Large', 'XLarge')
    ),
    PRIMARY KEY (catalog_version, platform_sku, size)
) STRICT;

CREATE TABLE components (
    catalog_version INTEGER NOT NULL REFERENCES catalog_revisions(version),
    sku TEXT NOT NULL,
    kind TEXT NOT NULL CHECK (
        kind IN (
            'Placard', 'BackPanel', 'CummerbundFrontPouch',
            'CummerbundRearPouch', 'BeltItem'
        )
    ),
    msrp_cents INTEGER NOT NULL CHECK (msrp_cents >= 0),
    spec_json TEXT NOT NULL,
    PRIMARY KEY (catalog_version, sku)
) STRICT;

CREATE TABLE adapter_bridges (
    catalog_version INTEGER NOT NULL REFERENCES catalog_revisions(version),
    sku TEXT NOT NULL,
    from_interface TEXT NOT NULL CHECK (
        from_interface IN (
            'SwiftClip1InchQasm', 'GhookLowProfile', 'CryeAvsFlap',
            'ShawArcQuad', 'VelcroFlap', 'FirstSpearTubes2In',
            'FirstSpearTubes4In', 'RocBuckle80', 'ShockCordLace',
            'InternalVelcroLoop', 'PassThroughSkeletal', 'ProprietaryTegris',
            'CryeZip10', 'FerroZip', 'SpiritusZip', 'VelocityYkk10',
            'StandardMolle', 'InnerHookOuterLoop', 'InnerLoopOuterHook',
            'StandardPals1In', 'MicroPals05In'
        )
    ),
    to_interface TEXT NOT NULL CHECK (
        to_interface IN (
            'SwiftClip1InchQasm', 'GhookLowProfile', 'CryeAvsFlap',
            'ShawArcQuad', 'VelcroFlap', 'FirstSpearTubes2In',
            'FirstSpearTubes4In', 'RocBuckle80', 'ShockCordLace',
            'InternalVelcroLoop', 'PassThroughSkeletal', 'ProprietaryTegris',
            'CryeZip10', 'FerroZip', 'SpiritusZip', 'VelocityYkk10',
            'StandardMolle', 'InnerHookOuterLoop', 'InnerLoopOuterHook',
            'StandardPals1In', 'MicroPals05In'
        )
    ),
    msrp_cents INTEGER NOT NULL CHECK (msrp_cents >= 0),
    spec_json TEXT NOT NULL,
    PRIMARY KEY (catalog_version, sku)
) STRICT;

CREATE TABLE vendor_offers (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    catalog_version INTEGER NOT NULL REFERENCES catalog_revisions(version),
    item_sku TEXT NOT NULL,
    vendor_name TEXT NOT NULL,
    price_cents INTEGER NOT NULL CHECK (price_cents > 0),
    in_stock INTEGER NOT NULL CHECK (in_stock IN (0, 1)),
    product_url TEXT NOT NULL
) STRICT;

CREATE INDEX idx_vendor_offers_sku ON vendor_offers (catalog_version, item_sku);

CREATE TABLE role_presets (
    catalog_version INTEGER NOT NULL REFERENCES catalog_revisions(version),
    preset_id TEXT NOT NULL,
    role TEXT CHECK (
        role IS NULL
        OR role IN ('breacher', 'jtac', 'drone_operator', 'sniper_dm', 'recce')
    ),
    policy TEXT NOT NULL CHECK (
        policy IN (
            'us_army_ar670_1', 'usmc_maradmin',
            'law_enforcement_patrol', 'law_enforcement_swat'
        )
    ),
    platform_sku TEXT NOT NULL,
    preset_json TEXT NOT NULL,
    PRIMARY KEY (catalog_version, preset_id)
) STRICT;

CREATE TABLE loadouts (
    id TEXT PRIMARY KEY,
    platform_sku TEXT NOT NULL,
    catalog_version INTEGER NOT NULL REFERENCES catalog_revisions(version),
    version INTEGER NOT NULL DEFAULT 1 CHECK (version >= 1),
    colorway TEXT NOT NULL CHECK (
        colorway IN (
            'multicam', 'coyote_brown', 'ranger_green', 'wolf_grey',
            'black', 'woodland_m81'
        )
    ),
    regulation_policy TEXT CHECK (
        regulation_policy IS NULL
        OR regulation_policy IN (
            'us_army_ar670_1', 'usmc_maradmin',
            'law_enforcement_patrol', 'law_enforcement_swat'
        )
    ),
    operational_role TEXT CHECK (
        operational_role IS NULL
        OR operational_role IN ('breacher', 'jtac', 'drone_operator', 'sniper_dm', 'recce')
    ),
    placements_json TEXT NOT NULL,
    schema_version INTEGER NOT NULL DEFAULT 1,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL
) STRICT;

CREATE INDEX idx_loadouts_updated ON loadouts (updated_at);

CREATE TABLE loadout_events (
    event_id INTEGER PRIMARY KEY AUTOINCREMENT,
    loadout_id TEXT NOT NULL REFERENCES loadouts(id) ON DELETE CASCADE,
    version INTEGER NOT NULL CHECK (version >= 1),
    event_type TEXT NOT NULL CHECK (
        event_type IN ('created', 'updated', 'deleted', 're_evaluated')
    ),
    payload_json TEXT NOT NULL,
    occurred_at TEXT NOT NULL
) STRICT;

CREATE INDEX idx_loadout_events_loadout ON loadout_events (loadout_id);
