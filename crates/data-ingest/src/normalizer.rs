//! Deterministic normalization of raw CSV strings into `core_engine` enums,
//! plus the seed generator for starter components and adapter bridges.
//!
//! Mapping rules (all deterministic, zero silent fallbacks):
//! - **Interface matching** uses per-column pattern tables. Among all patterns
//!   present in a cell, the **earliest mention in the text** wins — the Google
//!   Sheet convention lists the native standard first — with table order
//!   breaking exact-position ties. No match ⇒ `IngestError::UnmappedInterface`.
//! - **SKU ids** are `slug(Brand + " " + ProductName)`, e.g.
//!   `crye-precision-jpc-2-0`. Record `Uuid`s are DNS-namespace v5 hashes of
//!   the slug, so seeds are byte-identical across runs.
//! - **Orthographic slots** are generated per platform category with the
//!   canvas layout constants defined below.

use crate::csv_models::{
    RawBeltRow, RawChestRigRow, RawLbeRow, RawPlateCarrierRow, RawSchemaRuleRow,
};
use crate::{Catalog, IngestError, IngestReport};
use core_engine::interfaces::{InterfaceStandard, PlateCut, Size};
use core_engine::spatial::{AnchorBox2D, PalsMatrix};
use core_engine::uuid::Uuid;

// --- text normalization --------------------------------------------------------

/// Lowercase, de-quote, unify `&`→` and `, collapse whitespace.
fn norm(text: &str) -> String {
    let and_expanded = text.replace('&', " and ");
    and_expanded
        .chars()
        .map(|c| match c {
            '"' | '“' | '”' | '„' => ' ',
            _ => c,
        })
        .collect::<String>()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .to_lowercase()
}

/// Whether a cell explicitly states "no standard here" (empty / none / n/a).
fn is_explicit_none(text: &str) -> bool {
    let t = norm(text);
    t.is_empty() || t == "none" || t == "n/a" || t == "na" || t == "-"
}

/// Find the interface named earliest in `text`, using `table` (deterministic:
/// earliest position wins; table order breaks ties).
fn match_earliest(
    text: &str,
    table: &[(&str, InterfaceStandard)],
) -> Option<InterfaceStandard> {
    let hay = norm(text);
    table
        .iter()
        .enumerate()
        .filter_map(|(table_idx, (pat, std))| hay.find(pat).map(|pos| (pos, table_idx, *std)))
        .min_by_key(|(pos, table_idx, _)| (*pos, *table_idx))
        .map(|(_, _, std)| std)
}

/// Map a cell through `table`, failing loudly with row/column context.
fn map_interface(
    text: &str,
    table: &[(&str, InterfaceStandard)],
    row: usize,
    column: &str,
) -> Result<InterfaceStandard, IngestError> {
    match_earliest(text, table).ok_or_else(|| IngestError::UnmappedInterface {
        row,
        column: column.to_owned(),
        raw_value: text.trim().to_owned(),
    })
}

// --- per-column pattern tables --------------------------------------------------

/// Placard / detachable-front columns (also used for chest-rig main cells).
/// `pals`/`molle` map to `StandardPals1In` — a fixed webbing front.
const PLACARD_TABLE: &[(&str, InterfaceStandard)] = &[
    ("avs", InterfaceStandard::CryeAvsFlap),
    ("swiftclip", InterfaceStandard::SwiftClip1InchQasm),
    ("qasm", InterfaceStandard::SwiftClip1InchQasm),
    ("g-hook", InterfaceStandard::GhookLowProfile),
    ("ghook", InterfaceStandard::GhookLowProfile),
    ("arc quad", InterfaceStandard::ShawArcQuad),
    ("arc", InterfaceStandard::ShawArcQuad),
    ("shaw", InterfaceStandard::ShawArcQuad),
    ("pals", InterfaceStandard::StandardPals1In),
    ("molle", InterfaceStandard::StandardPals1In),
];

/// Cummerbund columns. Covers both front and rear cummerbund standards; the
/// matched standard defines the slot interface either way. Unqualified
/// "Tubes" deterministically means FirstSpear 2" Tubes.
const CUMMERBUND_TABLE: &[(&str, InterfaceStandard)] = &[
    ("internal velcro", InterfaceStandard::InternalVelcroLoop),
    ("internal loop", InterfaceStandard::InternalVelcroLoop),
    ("shock cord", InterfaceStandard::ShockCordLace),
    ("tegris", InterfaceStandard::ProprietaryTegris),
    ("pass-through", InterfaceStandard::PassThroughSkeletal),
    ("pass through", InterfaceStandard::PassThroughSkeletal),
    ("skeletal", InterfaceStandard::PassThroughSkeletal),
    ("tubes", InterfaceStandard::FirstSpearTubes2In),
    ("roc", InterfaceStandard::RocBuckle80),
    ("velcro flap", InterfaceStandard::VelcroFlap),
    ("hook and loop", InterfaceStandard::VelcroFlap),
    ("velcro", InterfaceStandard::VelcroFlap),
];

/// Back panel / zip-on columns.
const BACK_PANEL_TABLE: &[(&str, InterfaceStandard)] = &[
    ("crye", InterfaceStandard::CryeZip10),
    ("ferro", InterfaceStandard::FerroZip),
    ("spiritus", InterfaceStandard::SpiritusZip),
    ("velocity", InterfaceStandard::VelocityYkk10),
    ("ykk", InterfaceStandard::VelocityYkk10),
    ("pals", InterfaceStandard::StandardMolle),
    ("molle", InterfaceStandard::StandardMolle),
    ("loop pile", InterfaceStandard::StandardMolle),
];

/// Belt inner/outer hook-and-loop stack orientation columns.
const BELT_VELCRO_TABLE: &[(&str, InterfaceStandard)] = &[
    ("inner hook", InterfaceStandard::InnerHookOuterLoop),
    ("inner loop", InterfaceStandard::InnerLoopOuterHook),
    ("outer loop", InterfaceStandard::InnerHookOuterLoop),
    ("outer hook", InterfaceStandard::InnerLoopOuterHook),
];

/// Belt MOLLE/PALS mounting spec columns.
const PALS_SPEC_TABLE: &[(&str, InterfaceStandard)] = &[
    ("micro", InterfaceStandard::MicroPals05In),
    ("pals", InterfaceStandard::StandardPals1In),
    ("molle", InterfaceStandard::StandardPals1In),
];

/// Schema "Standard / Specification Name" column (any family).
const SCHEMA_STANDARD_TABLE: &[(&str, InterfaceStandard)] = &[
    ("swiftclip", InterfaceStandard::SwiftClip1InchQasm),
    ("qasm", InterfaceStandard::SwiftClip1InchQasm),
    ("avs flap", InterfaceStandard::CryeAvsFlap),
    ("avs", InterfaceStandard::CryeAvsFlap),
    ("g-hook", InterfaceStandard::GhookLowProfile),
    ("arc quad", InterfaceStandard::ShawArcQuad),
    ("arc", InterfaceStandard::ShawArcQuad),
    ("tegris", InterfaceStandard::ProprietaryTegris),
    ("shock cord", InterfaceStandard::ShockCordLace),
    ("internal velcro", InterfaceStandard::InternalVelcroLoop),
    ("internal loop", InterfaceStandard::InternalVelcroLoop),
    ("pass-through", InterfaceStandard::PassThroughSkeletal),
    ("skeletal", InterfaceStandard::PassThroughSkeletal),
    ("tubes", InterfaceStandard::FirstSpearTubes2In),
    ("roc", InterfaceStandard::RocBuckle80),
    ("velcro flap", InterfaceStandard::VelcroFlap),
    ("inner hook", InterfaceStandard::InnerHookOuterLoop),
    ("inner loop", InterfaceStandard::InnerLoopOuterHook),
    ("crye", InterfaceStandard::CryeZip10),
    ("ferro", InterfaceStandard::FerroZip),
    ("spiritus", InterfaceStandard::SpiritusZip),
    ("ykk", InterfaceStandard::VelocityYkk10),
    ("velocity", InterfaceStandard::VelocityYkk10),
    ("molle", InterfaceStandard::StandardMolle),
    ("pals", InterfaceStandard::StandardPals1In),
];

// --- ids ------------------------------------------------------------------------

/// Deterministic slug SKU: `slug(brand + " " + name)`.
pub fn slug_id(brand: &str, name: &str) -> String {
    slug::slugify(format!("{brand} {name}"))
}

/// Deterministic v5 (DNS namespace) UUID for a slug — identical on every run,
/// so generated seeds are reproducible.
pub fn deterministic_uuid(slug: &str) -> Uuid {
    Uuid::new_v5(&Uuid::NAMESPACE_DNS, slug.as_bytes())
}

/// Default size ladder assigned to every platform (the sheet has no size
/// column; the full ladder is the documented deterministic default).
fn default_sizes() -> Vec<Size> {
    vec![Size::Small, Size::Medium, Size::Large, Size::XLarge]
}

// --- plate cut parsing ------------------------------------------------------------

/// Parse a "Plate Cut / Sizing" cell into plate cuts. Unmapped non-empty
/// values fail loudly.
fn map_plate_cuts(text: &str, row: usize) -> Result<Vec<PlateCut>, IngestError> {
    if is_explicit_none(text) {
        return Ok(Vec::new());
    }
    let hay = norm(text);
    let mut cuts = Vec::new();
    if hay.contains("sapi") || hay.contains("esapi") || hay.contains("mbav") {
        cuts.push(PlateCut::SapiEsapi);
    }
    if hay.contains("swimmer") {
        cuts.push(PlateCut::Swimmer);
    }
    if hay.contains("shooter") || hay.contains("10x12") {
        cuts.push(PlateCut::Shooter);
    }
    if cuts.is_empty() {
        return Err(IngestError::UnmappedPlateCut {
            row,
            column: "Plate Cut / Sizing".to_owned(),
            raw_value: text.trim().to_owned(),
        });
    }
    Ok(cuts)
}

// --- orthographic slot generators --------------------------------------------------

/// Canvas geometry constants (percent of the platform canvas).
mod layout {
    use core_engine::spatial::AnchorBox2D;

    /// Full-slot anchor helper: child fills the slot box.
    pub const FULL: fn() -> AnchorBox2D = || AnchorBox2D::new(0.0, 0.0, 100.0, 100.0, 1);
    /// Front placard field.
    pub const FRONT_PLACARD: AnchorBox2D = AnchorBox2D::new(30.0, 18.0, 40.0, 46.0, 10);
    /// Zip-on back panel area.
    pub const REAR_PANEL: AnchorBox2D = AnchorBox2D::new(30.0, 20.0, 40.0, 44.0, 10);
    /// Rear MOLLE field.
    pub const REAR_PALS: AnchorBox2D = AnchorBox2D::new(30.0, 20.0, 40.0, 44.0, 9);
    /// Left cummerbund face.
    pub const CUMMERBUND_LEFT: AnchorBox2D = AnchorBox2D::new(5.0, 25.0, 20.0, 35.0, 8);
    /// Right cummerbund face.
    pub const CUMMERBUND_RIGHT: AnchorBox2D = AnchorBox2D::new(75.0, 25.0, 20.0, 35.0, 8);
    /// Dangler zone under the placard.
    pub const DANGLER: AnchorBox2D = AnchorBox2D::new(40.0, 66.0, 20.0, 12.0, 12);
    /// Belt PALS field.
    pub const BELT_PALS: AnchorBox2D = AnchorBox2D::new(2.0, 40.0, 96.0, 20.0, 5);
    /// Holster drop zone.
    pub const HOLSTER: AnchorBox2D = AnchorBox2D::new(12.0, 62.0, 18.0, 26.0, 6);
    /// Chest rig main cell.
    pub const RIG_MAIN: AnchorBox2D = AnchorBox2D::new(25.0, 20.0, 50.0, 42.0, 10);
    /// Left rig wing.
    pub const WING_LEFT: AnchorBox2D = AnchorBox2D::new(4.0, 24.0, 18.0, 34.0, 8);
    /// Right rig wing.
    pub const WING_RIGHT: AnchorBox2D = AnchorBox2D::new(78.0, 24.0, 18.0, 34.0, 8);
    /// LBE MOLLE field.
    pub const LBE_MAIN: AnchorBox2D = AnchorBox2D::new(15.0, 25.0, 70.0, 45.0, 6);
    /// LBE buttpack interface.
    pub const BUTTPACK: AnchorBox2D = AnchorBox2D::new(25.0, 72.0, 50.0, 20.0, 7);
}

use core_engine::graph::MountSlot;
use layout as lay;

/// Default plate-carrier slots: `front_placard` (6x3 PALS), `rear_panel`
/// (zipper) + `rear_pals_field` (6x4), both cummerbunds, and a dangler zone.
pub fn plate_carrier_slots(
    front_interface: InterfaceStandard,
    cummerbund_interface: InterfaceStandard,
    back_panel_interface: InterfaceStandard,
) -> Vec<MountSlot> {
    let mut front = MountSlot {
        id: "front_placard".into(),
        label: "Front placard field".into(),
        interface: front_interface,
        anchor: lay::FRONT_PLACARD,
        pals_grid: None,
    };
    if front_interface == InterfaceStandard::StandardPals1In {
        front.pals_grid = Some(PalsMatrix::new(6, 3));
    }
    vec![
        front,
        MountSlot {
            id: "rear_panel".into(),
            label: "Zip-on back panel".into(),
            interface: back_panel_interface,
            anchor: lay::REAR_PANEL,
            pals_grid: None,
        },
        MountSlot {
            id: "rear_pals_field".into(),
            label: "Rear MOLLE field".into(),
            interface: InterfaceStandard::StandardMolle,
            anchor: lay::REAR_PALS,
            pals_grid: Some(PalsMatrix::new(6, 4)),
        },
        MountSlot {
            id: "cummerbund_left".into(),
            label: "Cummerbund left".into(),
            interface: cummerbund_interface,
            anchor: lay::CUMMERBUND_LEFT,
            pals_grid: None,
        },
        MountSlot {
            id: "cummerbund_right".into(),
            label: "Cummerbund right".into(),
            interface: cummerbund_interface,
            anchor: lay::CUMMERBUND_RIGHT,
            pals_grid: None,
        },
        MountSlot {
            id: "dangler_slot".into(),
            label: "Dangler zone".into(),
            // Documented default: 1" SwiftClip is the community-standard
            // dangler mount (e.g. SPUD-style pouches).
            interface: InterfaceStandard::SwiftClip1InchQasm,
            anchor: lay::DANGLER,
            pals_grid: None,
        },
    ]
}

/// Default belt slots: `belt_outer_pals` (18x2) and a `holster_drop_zone`.
pub fn belt_slots(pals_interface: InterfaceStandard) -> Vec<MountSlot> {
    vec![
        MountSlot {
            id: "belt_outer_pals".into(),
            label: "Outer PALS field".into(),
            interface: pals_interface,
            anchor: lay::BELT_PALS,
            pals_grid: Some(PalsMatrix::new(18, 2)),
        },
        MountSlot {
            id: "holster_drop_zone".into(),
            label: "Holster drop zone".into(),
            interface: InterfaceStandard::StandardPals1In,
            anchor: lay::HOLSTER,
            pals_grid: Some(PalsMatrix::new(2, 3)),
        },
    ]
}

/// Default chest-rig slots: `rig_main_cell`, both wings, and a dangler zone.
/// Wings expose SwiftClip when the harness text mentions QASM/SwiftClip,
/// otherwise 1" PALS (deterministic rule).
pub fn chest_rig_slots(
    main_interface: InterfaceStandard,
    harness_text: &str,
    dangler_interface: InterfaceStandard,
) -> Vec<MountSlot> {
    let wing_interface = if norm(harness_text).contains("qasm") || norm(harness_text).contains("swiftclip") {
        InterfaceStandard::SwiftClip1InchQasm
    } else {
        InterfaceStandard::StandardPals1In
    };
    let mut main = MountSlot {
        id: "rig_main_cell".into(),
        label: "Main cell".into(),
        interface: main_interface,
        anchor: lay::RIG_MAIN,
        pals_grid: None,
    };
    if main_interface == InterfaceStandard::StandardPals1In {
        main.pals_grid = Some(PalsMatrix::new(6, 3));
    }
    vec![
        main,
        MountSlot {
            id: "wing_left".into(),
            label: "Left wing".into(),
            interface: wing_interface,
            anchor: lay::WING_LEFT,
            pals_grid: None,
        },
        MountSlot {
            id: "wing_right".into(),
            label: "Right wing".into(),
            interface: wing_interface,
            anchor: lay::WING_RIGHT,
            pals_grid: None,
        },
        MountSlot {
            id: "dangler_slot".into(),
            label: "Sub-load / dangler zone".into(),
            interface: dangler_interface,
            anchor: lay::DANGLER,
            pals_grid: None,
        },
    ]
}

/// Default LBE slots: a `lbe_main_field` (8x4 PALS) and a buttpack interface.
pub fn lbe_slots(
    main_interface: InterfaceStandard,
    buttpack_interface: InterfaceStandard,
) -> Vec<MountSlot> {
    let mut main = MountSlot {
        id: "lbe_main_field".into(),
        label: "MOLLE sustainment field".into(),
        interface: main_interface,
        anchor: lay::LBE_MAIN,
        pals_grid: None,
    };
    if main_interface == InterfaceStandard::StandardPals1In {
        main.pals_grid = Some(PalsMatrix::new(8, 4));
    }
    vec![
        main,
        MountSlot {
            id: "buttpack_interface".into(),
            label: "Buttpack / field pack mount".into(),
            interface: buttpack_interface,
            anchor: lay::BUTTPACK,
            pals_grid: None,
        },
    ]
}

// --- row normalizers ------------------------------------------------------------

use core_engine::graph::{ComponentKind, ComponentSpec, PlatformCategory, PlatformSpec};

/// Resolve the pricing/colorway seed for a platform SKU, failing loudly
/// when the table has no entry (zero silent fallbacks — R2).
fn platform_pricing_or_fail(
    row_no: usize,
    source: &'static str,
    sku: &str,
) -> Result<crate::pricing_seed::ItemPricing, IngestError> {
    crate::pricing_seed::platform_pricing(sku).ok_or_else(|| IngestError::MissingPricing {
        row: row_no,
        sheet: source,
        sku: sku.to_owned(),
    })
}

/// Normalize one plate-carrier row into a `PlatformSpec`.
pub fn normalize_plate_carrier(row: &RawPlateCarrierRow, row_no: usize) -> Result<PlatformSpec, IngestError> {
    let slug = slug_id(&row.brand, &row.platform_name);
    let front = map_interface(&row.front_placard_interface, PLACARD_TABLE, row_no, "Front Placard Interface")?;
    let cumb = map_interface(&row.cummerbund_interface, CUMMERBUND_TABLE, row_no, "Cummerbund Interface")?;
    let back = map_interface(&row.back_panel_zipper_standard, BACK_PANEL_TABLE, row_no, "Back Panel / Zipper Standard")?;
    let pricing = platform_pricing_or_fail(row_no, "plate_carriers.csv", &slug)?;
    Ok(PlatformSpec {
        id: deterministic_uuid(&slug),
        sku: slug,
        name: row.platform_name.trim().to_owned(),
        category: Some(PlatformCategory::PlateCarrier),
        plate_cuts: map_plate_cuts(&row.plate_cut_sizing, row_no)?,
        sizes: default_sizes(),
        slots: plate_carrier_slots(front, cumb, back),
        msrp_cents: pricing.msrp_cents,
        vendor_offers: pricing.offers,
        allowed_colorways: pricing.allowed_colorways,
        default_colorway: pricing.default_colorway,
    })
}

/// Normalize one belt row into a `PlatformSpec`.
pub fn normalize_belt(row: &RawBeltRow, row_no: usize) -> Result<PlatformSpec, IngestError> {
    let slug = slug_id(&row.brand, &row.belt_name);
    let _velcro = map_interface(&row.inner_outer_velcro_spec, BELT_VELCRO_TABLE, row_no, "Inner/Outer Velcro Spec")?;
    let pals = map_interface(&row.molle_pals_mounting_spec, PALS_SPEC_TABLE, row_no, "MOLLE / PALS Mounting Spec")?;
    let _holster = map_interface(&row.holster_mounting_compatibility, PALS_SPEC_TABLE, row_no, "Holster Mounting Compatibility")?;
    let pricing = platform_pricing_or_fail(row_no, "belts.csv", &slug)?;
    Ok(PlatformSpec {
        id: deterministic_uuid(&slug),
        sku: slug,
        name: row.belt_name.trim().to_owned(),
        category: Some(PlatformCategory::Belt),
        plate_cuts: Vec::new(),
        sizes: default_sizes(),
        slots: belt_slots(pals),
        msrp_cents: pricing.msrp_cents,
        vendor_offers: pricing.offers,
        allowed_colorways: pricing.allowed_colorways,
        default_colorway: pricing.default_colorway,
    })
}

/// Normalize one chest-rig row into a `PlatformSpec`.
pub fn normalize_chest_rig(row: &RawChestRigRow, row_no: usize) -> Result<PlatformSpec, IngestError> {
    let slug = slug_id(&row.brand, &row.rig_name);
    let main = map_interface(&row.harness_buckle_interface, PLACARD_TABLE, row_no, "Harness & Buckle Interface")?;
    // Explicit "None" sub-load cells fall back to the documented dangler
    // default (SwiftClip); any non-empty value must map deterministically.
    let dangler = if is_explicit_none(&row.sub_load_dangler_interface) {
        InterfaceStandard::SwiftClip1InchQasm
    } else {
        map_interface(&row.sub_load_dangler_interface, PLACARD_TABLE, row_no, "Sub-Load / Dangler Interface")?
    };
    let pricing = platform_pricing_or_fail(row_no, "chest_rigs.csv", &slug)?;
    Ok(PlatformSpec {
        id: deterministic_uuid(&slug),
        sku: slug,
        name: row.rig_name.trim().to_owned(),
        category: Some(PlatformCategory::ChestRig),
        plate_cuts: Vec::new(),
        sizes: default_sizes(),
        slots: chest_rig_slots(main, &row.harness_buckle_interface, dangler),
        msrp_cents: pricing.msrp_cents,
        vendor_offers: pricing.offers,
        allowed_colorways: pricing.allowed_colorways,
        default_colorway: pricing.default_colorway,
    })
}

/// Normalize one LBE row into a `PlatformSpec`.
pub fn normalize_lbe(row: &RawLbeRow, row_no: usize) -> Result<PlatformSpec, IngestError> {
    let slug = slug_id(&row.brand, &row.lbe_system_name);
    let main = map_interface(&row.integrated_vs_molle_pouches, PALS_SPEC_TABLE, row_no, "Integrated vs. MOLLE Pouches")?;
    let buttpack = map_interface(&row.buttpack_field_pack_integration, BACK_PANEL_TABLE, row_no, "Buttpack / Field Pack Integration")?;
    let pricing = platform_pricing_or_fail(row_no, "lbe.csv", &slug)?;
    Ok(PlatformSpec {
        id: deterministic_uuid(&slug),
        sku: slug,
        name: row.lbe_system_name.trim().to_owned(),
        category: Some(PlatformCategory::Lbe),
        plate_cuts: Vec::new(),
        sizes: default_sizes(),
        slots: lbe_slots(main, buttpack),
        msrp_cents: pricing.msrp_cents,
        vendor_offers: pricing.offers,
        allowed_colorways: pricing.allowed_colorways,
        default_colorway: pricing.default_colorway,
    })
}

/// Validate schema rule rows: every named standard must map to an
/// `InterfaceStandard`, and every referenced bridge must exist in the seed
/// adapter set.
pub fn validate_schema_rows(rows: &[RawSchemaRuleRow], known_adapter_slugs: &[&str]) -> Result<(), IngestError> {
    for (idx, row) in rows.iter().enumerate() {
        let row_no = idx + 1;
        map_interface(&row.standard_specification_name, SCHEMA_STANDARD_TABLE, row_no, "Standard / Specification Name")?;
        let bridge = norm(&row.required_bridge_adapter_solution);
        if !is_explicit_none(&row.required_bridge_adapter_solution)
            && !bridge.contains("native")
            && !bridge.contains("none")
            && !known_adapter_slugs.iter().any(|slug| bridge.contains(slug))
        {
            return Err(IngestError::UnmappedInterface {
                row: row_no,
                column: "Required Bridge / Adapter Solution".to_owned(),
                raw_value: row.required_bridge_adapter_solution.trim().to_owned(),
            });
        }
    }
    Ok(())
}

// --- seed components and adapters -------------------------------------------------

/// The essential aftermarket bridges referenced by the compatibility schema.
/// Bridge direction: `platform_interface` attaches to the platform,
/// `component_interface` is exposed to the child component.
pub fn seed_core_adapters_and_components() -> (Vec<core_engine::graph::AdapterBridge>, Vec<ComponentSpec>) {
    let adapters = vec![
        adapter(
            "axl-avp-jpc",
            "AXL AVP (AVS Vertical Placard) Adapter",
            InterfaceStandard::CryeAvsFlap,
            InterfaceStandard::SwiftClip1InchQasm,
        ),
        adapter(
            "axl-eclipse-velcro-adapter",
            "AXL Eclipse Velcro Placard Adapter",
            InterfaceStandard::VelcroFlap,
            InterfaceStandard::SwiftClip1InchQasm,
        ),
        adapter(
            "ferro-3ac-tubes-adapter",
            "Ferro 3AC Tubes Cummerbund Adapter",
            InterfaceStandard::FirstSpearTubes2In,
            InterfaceStandard::SwiftClip1InchQasm,
        ),
        adapter(
            "ghook-to-qasm-buckle-kit",
            "G-Hook to QASM SwiftClip Buckle Kit",
            InterfaceStandard::GhookLowProfile,
            InterfaceStandard::SwiftClip1InchQasm,
        ),
    ];

    let components = vec![
        component(
            "spiritus-mk5-placard",
            "Spiritus Systems MK5 Placard",
            ComponentKind::Placard,
            InterfaceStandard::SwiftClip1InchQasm,
            None,
            vec![placard_front_pals()],
        ),
        component(
            "ferro-dope-flap",
            "Ferro Concepts DOPE Flap",
            ComponentKind::Placard,
            InterfaceStandard::GhookLowProfile,
            None,
            Vec::new(),
        ),
        component(
            "haley-d3crm-placard",
            "Haley Strategic D3CR Micro Placard",
            ComponentKind::Placard,
            InterfaceStandard::SwiftClip1InchQasm,
            None,
            Vec::new(),
        ),
        component(
            "esstac-kywi-556-double",
            "Esstac KYWI 5.56 Double",
            ComponentKind::BeltItem,
            InterfaceStandard::StandardPals1In,
            Some((2, 2)),
            Vec::new(),
        ),
        component(
            "spiritus-spud-pouch",
            "Spiritus Systems SPUD Pouch",
            ComponentKind::BeltItem,
            InterfaceStandard::SwiftClip1InchQasm,
            None,
            Vec::new(),
        ),
        component(
            "ferro-roll-1-ifak",
            "Ferro Concepts Roll-1 IFAK",
            ComponentKind::BeltItem,
            InterfaceStandard::StandardPals1In,
            Some((2, 2)),
            Vec::new(),
        ),
        component(
            "crye-zip-on-pack-2-0",
            "Crye Zip-On Pack 2.0",
            ComponentKind::BackPanel,
            InterfaceStandard::CryeZip10,
            None,
            Vec::new(),
        ),
        component(
            "ferro-back-panel-banger",
            "Ferro Concepts Back Panel Banger",
            ComponentKind::BackPanel,
            InterfaceStandard::FerroZip,
            None,
            Vec::new(),
        ),
        component(
            "safariland-ubl-mid-ride",
            "Safariland UBL Mid-Ride",
            ComponentKind::BeltItem,
            InterfaceStandard::StandardPals1In,
            Some((2, 3)),
            Vec::new(),
        ),
        component(
            "bfg-micro-trauma-now",
            "BFG Micro Trauma NOW!",
            ComponentKind::BeltItem,
            InterfaceStandard::SwiftClip1InchQasm,
            Some((4, 3)),
            Vec::new(),
        ),
        // Spec-04 additions: typed comms/PTT and admin/ATAK carriage, so the
        // JTAC and drone-operator role checks have catalog coverage.
        component(
            "agilite-ptt-3-panel",
            "Agilite PTT-3 Comms Panel",
            ComponentKind::BeltItem,
            InterfaceStandard::StandardPals1In,
            Some((3, 2)),
            Vec::new(),
        ),
        component(
            "ferro-concepts-admin-pouch",
            "Ferro Concepts Admin Pouch",
            ComponentKind::BeltItem,
            InterfaceStandard::StandardPals1In,
            Some((2, 2)),
            Vec::new(),
        ),
    ];

    (adapters, components)
}

/// Adapter convenience constructor with deterministic id, default hardware
/// footprint anchor, and pricing/colorways from the seed table.
fn adapter(
    slug: &str,
    name: &str,
    platform_interface: InterfaceStandard,
    component_interface: InterfaceStandard,
) -> core_engine::graph::AdapterBridge {
    let pricing = crate::pricing_seed::item_offer_pricing(slug)
        .unwrap_or_else(|| panic!("pricing seed table must cover adapter `{slug}`"));
    core_engine::graph::AdapterBridge {
        id: deterministic_uuid(slug),
        sku: slug.to_owned(),
        name: name.to_owned(),
        platform_interface,
        component_interface,
        // Adapter hardware sits at the top of the slot; the child hangs below.
        anchor: AnchorBox2D::new(0.0, 0.0, 100.0, 6.0, 1),
        msrp_cents: pricing.msrp_cents,
        vendor_offers: pricing.offers,
        allowed_colorways: pricing.allowed_colorways,
        default_colorway: pricing.default_colorway,
    }
}

/// Component convenience constructor with deterministic id, full-slot
/// anchor, pricing/colorways, and typed capabilities from the seed table.
fn component(
    slug: &str,
    name: &str,
    kind: ComponentKind,
    interface: InterfaceStandard,
    footprint: Option<(u8, u8)>,
    child_slots: Vec<MountSlot>,
) -> ComponentSpec {
    let pricing = crate::pricing_seed::item_offer_pricing(slug)
        .unwrap_or_else(|| panic!("pricing seed table must cover component `{slug}`"));
    ComponentSpec {
        id: deterministic_uuid(slug),
        sku: slug.to_owned(),
        name: name.to_owned(),
        kind,
        interface,
        anchor: lay::FULL(),
        pals_footprint: footprint.map(|(cols, rows)| core_engine::graph::PalsFootprint { cols, rows }),
        child_slots,
        msrp_cents: pricing.msrp_cents,
        vendor_offers: pricing.offers,
        allowed_colorways: pricing.allowed_colorways,
        default_colorway: pricing.default_colorway,
        capabilities: crate::pricing_seed::capabilities(slug),
    }
}

/// Child slot exposed by the MK5 placard once mounted: a 6x3 front PALS
/// field. The anchor is relative to the placard's render anchor.
fn placard_front_pals() -> MountSlot {
    MountSlot {
        id: "placard_front_pals".into(),
        label: "Placard front PALS field".into(),
        interface: InterfaceStandard::StandardPals1In,
        anchor: AnchorBox2D::new(4.0, 10.0, 92.0, 82.0, 4),
        pals_grid: Some(PalsMatrix::new(6, 3)),
    }
}

/// Slugs of the seed adapters (used for schema bridge validation).
pub fn core_adapter_slugs() -> Vec<&'static str> {
    vec![
        "axl-avp-jpc",
        "axl-eclipse-velcro-adapter",
        "ferro-3ac-tubes-adapter",
        "ghook-to-qasm-buckle-kit",
    ]
}

// --- pipeline entry point -----------------------------------------------------------

/// Ingest all five CSVs from `input_dir` into a normalized catalog.
///
/// Fails with a descriptive [`IngestError`] on the first unmappable cell —
/// no row is ever silently dropped or guessed.
pub fn normalize_catalog(input_dir: &std::path::Path) -> Result<IngestReport, IngestError> {
    let carriers: Vec<RawPlateCarrierRow> = crate::read_csv(input_dir, "plate_carriers.csv")?;
    let belts: Vec<RawBeltRow> = crate::read_csv(input_dir, "belts.csv")?;
    let rigs: Vec<RawChestRigRow> = crate::read_csv(input_dir, "chest_rigs.csv")?;
    let lbe: Vec<RawLbeRow> = crate::read_csv(input_dir, "lbe.csv")?;
    let schema: Vec<RawSchemaRuleRow> = crate::read_csv(input_dir, "compatibility_schema.csv")?;

    let mut platforms = Vec::new();
    for (idx, row) in carriers.iter().enumerate() {
        platforms.push(normalize_plate_carrier(row, idx + 1)?);
    }
    for (idx, row) in belts.iter().enumerate() {
        platforms.push(normalize_belt(row, idx + 1)?);
    }
    for (idx, row) in rigs.iter().enumerate() {
        platforms.push(normalize_chest_rig(row, idx + 1)?);
    }
    for (idx, row) in lbe.iter().enumerate() {
        platforms.push(normalize_lbe(row, idx + 1)?);
    }

    let (adapters, components) = seed_core_adapters_and_components();
    validate_schema_rows(&schema, &core_adapter_slugs())?;

    Ok(IngestReport {
        catalog: Catalog {
            platforms,
            components,
            adapter_bridges: adapters,
            role_presets: crate::pricing_seed::role_presets(),
        },
        rows_ingested: vec![
            ("plate_carriers.csv".into(), carriers.len()),
            ("belts.csv".into(), belts.len()),
            ("chest_rigs.csv".into(), rigs.len()),
            ("lbe.csv".into(), lbe.len()),
            ("compatibility_schema.csv".into(), schema.len()),
        ],
    })
}
