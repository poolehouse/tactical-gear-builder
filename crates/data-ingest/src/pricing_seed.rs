//! Deterministic seed pricing, colorway, capability, and role-preset data.
//!
//! The Google Sheet CSVs carry no pricing columns, so MSRP, vendor offers,
//! and colorways live here as a SKU-keyed seed table — the same philosophy
//! as the synthesized starter components: explicit, reviewed, deterministic
//! data rather than guesses. Lookups fail loudly
//! ([`crate::IngestError::MissingPricing`]) instead of defaulting: a CSV row
//! without a matching entry is a data bug, not a zero price (R2).
//!
//! All prices are integer US cents. Vendor offers are realistic street
//! listings; product URLs are display-only strings and never fetched by the
//! demo (R8).

use core_engine::graph::CapabilityTag;
use core_engine::pricing::{Colorway, VendorOffer};
use core_engine::regulation::{
    OperationalRole, PresetPlacement, RegulationPolicy, RolePreset,
};

/// One vendor listing: (vendor name, price in US cents, in stock).
type OfferSpec = (&'static str, u32, bool);
/// One priced item: (MSRP cents, offered colorways, default colorway, offers).
type ItemSpec = (u32, &'static [Colorway], Colorway, &'static [OfferSpec]);

/// Known vendor display names and their storefront domains, in a fixed
/// order. URLs are derived deterministically from (vendor, sku).
const VENDOR_DOMAINS: &[(&str, &str)] = &[
    ("SKD Tactical", "www.skdtactical.com"),
    ("Optactical", "www.optactical.com"),
    ("Primary Arms", "www.primaryarms.com"),
    ("US Patriot Tactical", "www.uspatriottactical.com"),
    ("Gritr Sports", "www.gritrsports.com"),
    ("Vingtqlc Ops", "www.vingtqlops.com"),
];

/// Deterministic storefront URL for one offer.
fn vendor_url(vendor: &str, sku: &str) -> String {
    let domain = VENDOR_DOMAINS
        .iter()
        .find(|(name, _)| *name == vendor)
        .map(|(_, domain)| *domain)
        .expect("vendor domain table covers every seeded vendor");
    format!("https://{domain}/products/{sku}")
}

/// Expand offer specs into full [`VendorOffer`] records for `sku`.
fn offers(sku: &str, spec: &[OfferSpec]) -> Vec<VendorOffer> {
    spec.iter()
        .map(|(vendor, price_cents, in_stock)| VendorOffer {
            vendor_name: (*vendor).to_owned(),
            price_cents: *price_cents,
            in_stock: *in_stock,
            product_url: vendor_url(vendor, sku),
        })
        .collect()
}

/// Pricing for every seeded platform, keyed by SKU slug.
///
/// Realistic 2025-era US MSRPs in cents.
const PLATFORM_SPECS: &[(&str, ItemSpec)] = &[
    // --- plate carriers ------------------------------------------------------
    (
        "crye-precision-jpc-2-0",
        (
            29_900,
            &[Colorway::Multicam, Colorway::CoyoteBrown, Colorway::RangerGreen, Colorway::Black],
            Colorway::Multicam,
            &[("SKD Tactical", 26_499, true), ("Optactical", 27_499, true), ("US Patriot Tactical", 28_900, false)],
        ),
    ),
    (
        "crye-precision-airlite-spc",
        (
            24_900,
            &[Colorway::Multicam, Colorway::CoyoteBrown, Colorway::RangerGreen, Colorway::Black],
            Colorway::Multicam,
            &[("SKD Tactical", 21_999, true), ("Primary Arms", 23_400, true)],
        ),
    ),
    (
        "crye-precision-avs",
        (
            59_900,
            &[Colorway::Multicam, Colorway::CoyoteBrown, Colorway::RangerGreen],
            Colorway::Multicam,
            &[("SKD Tactical", 54_900, true), ("Optactical", 56_900, true)],
        ),
    ),
    (
        "ferro-concepts-fcpc-v5",
        (
            32_500,
            &[Colorway::Multicam, Colorway::CoyoteBrown, Colorway::RangerGreen],
            Colorway::Multicam,
            &[("SKD Tactical", 29_499, true), ("Gritr Sports", 30_900, true), ("US Patriot Tactical", 31_750, false)],
        ),
    ),
    (
        "ferro-concepts-the-slickster",
        (
            21_500,
            &[Colorway::Multicam, Colorway::CoyoteBrown, Colorway::RangerGreen, Colorway::WolfGrey, Colorway::Black],
            Colorway::Multicam,
            &[("Optactical", 18_999, true), ("Primary Arms", 20_100, true)],
        ),
    ),
    (
        "spiritus-systems-lv-119-overt",
        (
            27_400,
            &[Colorway::Multicam, Colorway::CoyoteBrown, Colorway::RangerGreen, Colorway::Black],
            Colorway::Multicam,
            &[("SKD Tactical", 24_499, true), ("Gritr Sports", 25_900, true)],
        ),
    ),
    (
        "spiritus-systems-lv-119-covert",
        (
            26_400,
            &[Colorway::Multicam, Colorway::CoyoteBrown, Colorway::RangerGreen],
            Colorway::CoyoteBrown,
            &[("Optactical", 23_499, true), ("Vingtqlc Ops", 24_900, true)],
        ),
    ),
    (
        "velocity-systems-scarab-lt",
        (
            28_900,
            &[Colorway::Multicam, Colorway::CoyoteBrown, Colorway::RangerGreen],
            Colorway::Multicam,
            &[("SKD Tactical", 25_999, true), ("Primary Arms", 27_200, true)],
        ),
    ),
    (
        "mayflower-apc",
        (
            24_900,
            &[Colorway::Multicam, Colorway::CoyoteBrown, Colorway::RangerGreen],
            Colorway::Multicam,
            &[("Optactical", 21_999, true), ("Gritr Sports", 23_100, false)],
        ),
    ),
    (
        "shaw-concepts-arc-v2",
        (
            27_900,
            &[Colorway::Multicam, Colorway::CoyoteBrown, Colorway::RangerGreen, Colorway::Black],
            Colorway::Multicam,
            &[("SKD Tactical", 24_999, true), ("Vingtqlc Ops", 26_300, true)],
        ),
    ),
    (
        "agilite-k19-3-0",
        (
            25_900,
            &[Colorway::Multicam, Colorway::CoyoteBrown, Colorway::RangerGreen, Colorway::Black, Colorway::WolfGrey],
            Colorway::Multicam,
            &[("Gritr Sports", 22_999, true), ("US Patriot Tactical", 24_200, true), ("Primary Arms", 25_400, false)],
        ),
    ),
    (
        "agilite-k-zero",
        (
            21_900,
            &[Colorway::Multicam, Colorway::CoyoteBrown, Colorway::Black],
            Colorway::CoyoteBrown,
            &[("Gritr Sports", 19_299, true), ("Optactical", 20_500, true)],
        ),
    ),
    (
        "defense-mechanisms-mepc",
        (
            23_900,
            &[Colorway::Multicam, Colorway::CoyoteBrown, Colorway::RangerGreen],
            Colorway::Multicam,
            &[("SKD Tactical", 21_299, true), ("Primary Arms", 22_400, true)],
        ),
    ),
    (
        "firstspear-strandhogg-v2",
        (
            30_900,
            &[Colorway::Multicam, Colorway::CoyoteBrown, Colorway::RangerGreen],
            Colorway::Multicam,
            &[("Optactical", 27_499, true), ("Vingtqlc Ops", 28_900, true), ("US Patriot Tactical", 29_950, false)],
        ),
    ),
    (
        "london-bridge-trading-lbt-6094a-g3",
        (
            30_900,
            &[Colorway::CoyoteBrown, Colorway::Multicam, Colorway::RangerGreen, Colorway::Black],
            Colorway::CoyoteBrown,
            &[("SKD Tactical", 27_999, true), ("Gritr Sports", 29_200, true)],
        ),
    ),
    (
        "hrt-tactical-hrac",
        (
            24_900,
            &[Colorway::Multicam, Colorway::CoyoteBrown, Colorway::Black, Colorway::RangerGreen],
            Colorway::Multicam,
            &[("Optactical", 21_999, true), ("US Patriot Tactical", 23_400, false)],
        ),
    ),
    (
        "haley-strategic-thorax",
        (
            30_400,
            &[Colorway::Multicam, Colorway::CoyoteBrown, Colorway::RangerGreen],
            Colorway::Multicam,
            &[("SKD Tactical", 27_499, true), ("Primary Arms", 28_600, true)],
        ),
    ),
    (
        "warrior-assault-systems-dcs",
        (
            23_900,
            &[Colorway::CoyoteBrown, Colorway::Multicam, Colorway::RangerGreen],
            Colorway::CoyoteBrown,
            &[("Gritr Sports", 20_999, true), ("Vingtqlc Ops", 22_300, true)],
        ),
    ),
    // --- belts ----------------------------------------------------------------
    (
        "aws-smu-belt",
        (
            10_500,
            &[Colorway::Black, Colorway::CoyoteBrown, Colorway::Multicam, Colorway::RangerGreen],
            Colorway::Black,
            &[("SKD Tactical", 8_999, true), ("Optactical", 9_750, true), ("Gritr Sports", 10_100, false)],
        ),
    ),
    (
        "ferro-concepts-bison-belt",
        (
            12_900,
            &[Colorway::Black, Colorway::CoyoteBrown, Colorway::RangerGreen],
            Colorway::Black,
            &[("Optactical", 11_499, true), ("Primary Arms", 12_100, true)],
        ),
    ),
    (
        "ronin-tactics-sensor-belt",
        (
            17_900,
            &[Colorway::Black, Colorway::CoyoteBrown, Colorway::RangerGreen, Colorway::Multicam],
            Colorway::Black,
            &[("SKD Tactical", 15_999, true), ("Vingtqlc Ops", 16_800, true)],
        ),
    ),
    (
        "agilite-battle-belt",
        (
            11_900,
            &[Colorway::Black, Colorway::Multicam, Colorway::CoyoteBrown, Colorway::RangerGreen],
            Colorway::Black,
            &[("Gritr Sports", 9_999, true), ("US Patriot Tactical", 10_900, true), ("Optactical", 11_400, false)],
        ),
    ),
    // --- chest rigs -----------------------------------------------------------
    (
        "crye-precision-mrb-2-0",
        (
            39_900,
            &[Colorway::Multicam, Colorway::CoyoteBrown, Colorway::RangerGreen],
            Colorway::Multicam,
            &[("SKD Tactical", 36_499, true), ("Primary Arms", 37_800, true)],
        ),
    ),
    (
        "mayflower-uw-gen-iv-chest-rig",
        (
            29_900,
            &[Colorway::CoyoteBrown, Colorway::Multicam, Colorway::RangerGreen, Colorway::WoodlandM81],
            Colorway::CoyoteBrown,
            &[("Optactical", 26_999, true), ("Gritr Sports", 28_200, false)],
        ),
    ),
    (
        "t-rex-arms-ac1",
        (
            14_900,
            &[Colorway::Black, Colorway::Multicam, Colorway::CoyoteBrown, Colorway::RangerGreen],
            Colorway::Multicam,
            &[("Optactical", 12_999, true), ("Primary Arms", 13_800, true)],
        ),
    ),
    (
        "spiritus-systems-micro-fight-mk4",
        (
            16_400,
            &[Colorway::Multicam, Colorway::CoyoteBrown, Colorway::RangerGreen, Colorway::Black],
            Colorway::Multicam,
            &[("SKD Tactical", 14_499, true), ("Vingtqlc Ops", 15_300, true)],
        ),
    ),
    // --- LBE ------------------------------------------------------------------
    (
        "jayjays-gen-4-brecon",
        (
            18_900,
            &[Colorway::CoyoteBrown, Colorway::Multicam, Colorway::WoodlandM81],
            Colorway::CoyoteBrown,
            &[("Optactical", 16_999, true), ("SKD Tactical", 17_750, true)],
        ),
    ),
    (
        "usgi-molle-ii-ruck",
        (
            14_900,
            &[Colorway::WoodlandM81, Colorway::CoyoteBrown, Colorway::Multicam],
            Colorway::WoodlandM81,
            // Surplus: every listing is out of stock, so the effective price
            // falls back to MSRP with zero savings (the documented rule).
            &[("Gritr Sports", 12_500, false), ("US Patriot Tactical", 13_200, false), ("Vingtqlc Ops", 13_900, false)],
        ),
    ),
    (
        "london-bridge-trading-lbv",
        (
            11_900,
            &[Colorway::CoyoteBrown, Colorway::Multicam, Colorway::WoodlandM81, Colorway::Black],
            Colorway::CoyoteBrown,
            &[("Optactical", 10_499, true), ("Gritr Sports", 11_100, true)],
        ),
    ),
];

/// Pricing for every seeded component and adapter, keyed by SKU slug.
const ITEM_SPECS: &[(&str, ItemSpec)] = &[
    (
        "spiritus-mk5-placard",
        (
            14_000,
            &[Colorway::Multicam, Colorway::CoyoteBrown, Colorway::RangerGreen, Colorway::Black],
            Colorway::Multicam,
            &[("SKD Tactical", 12_499, true), ("Optactical", 13_200, true)],
        ),
    ),
    (
        "ferro-dope-flap",
        (
            8_500,
            &[Colorway::Multicam, Colorway::CoyoteBrown, Colorway::RangerGreen],
            Colorway::Multicam,
            &[("Optactical", 7_499, true), ("Gritr Sports", 7_999, true)],
        ),
    ),
    (
        "haley-d3crm-placard",
        (
            20_000,
            &[Colorway::Multicam, Colorway::CoyoteBrown, Colorway::RangerGreen, Colorway::Black],
            Colorway::Multicam,
            &[("SKD Tactical", 17_999, true), ("Primary Arms", 18_800, true), ("US Patriot Tactical", 19_500, false)],
        ),
    ),
    (
        "esstac-kywi-556-double",
        (
            5_500,
            &[Colorway::Multicam, Colorway::CoyoteBrown, Colorway::RangerGreen, Colorway::Black],
            Colorway::Multicam,
            &[("Optactical", 4_799, true), ("SKD Tactical", 4_999, true), ("Gritr Sports", 5_250, false)],
        ),
    ),
    (
        "spiritus-spud-pouch",
        (
            6_500,
            &[Colorway::Multicam, Colorway::CoyoteBrown, Colorway::RangerGreen],
            Colorway::Multicam,
            &[("SKD Tactical", 5_799, true), ("Vingtqlc Ops", 6_100, true)],
        ),
    ),
    (
        "ferro-roll-1-ifak",
        (
            11_500,
            &[Colorway::Multicam, Colorway::CoyoteBrown, Colorway::RangerGreen, Colorway::Black],
            Colorway::Multicam,
            &[("Optactical", 10_199, true), ("Primary Arms", 10_800, true)],
        ),
    ),
    (
        "crye-zip-on-pack-2-0",
        (
            25_000,
            &[Colorway::Multicam, Colorway::CoyoteBrown, Colorway::RangerGreen],
            Colorway::Multicam,
            &[("SKD Tactical", 22_499, true), ("Gritr Sports", 23_600, true)],
        ),
    ),
    (
        "ferro-back-panel-banger",
        (
            15_000,
            &[Colorway::Multicam, Colorway::CoyoteBrown, Colorway::RangerGreen],
            Colorway::Multicam,
            &[("Optactical", 13_299, true), ("US Patriot Tactical", 14_100, false)],
        ),
    ),
    (
        "safariland-ubl-mid-ride",
        (
            6_200,
            &[Colorway::Black, Colorway::CoyoteBrown, Colorway::Multicam],
            Colorway::Black,
            &[("Optactical", 5_499, true), ("Primary Arms", 5_799, true), ("Gritr Sports", 5_950, false)],
        ),
    ),
    (
        "bfg-micro-trauma-now",
        (
            9_500,
            &[Colorway::Multicam, Colorway::CoyoteBrown, Colorway::RangerGreen, Colorway::Black],
            Colorway::Multicam,
            &[("SKD Tactical", 8_399, true), ("Vingtqlc Ops", 8_900, true)],
        ),
    ),
    (
        "agilite-ptt-3-panel",
        (
            7_000,
            &[Colorway::Multicam, Colorway::CoyoteBrown, Colorway::RangerGreen, Colorway::Black],
            Colorway::Multicam,
            &[("Gritr Sports", 6_199, true), ("US Patriot Tactical", 6_550, true)],
        ),
    ),
    (
        "ferro-concepts-admin-pouch",
        (
            5_000,
            &[Colorway::Multicam, Colorway::CoyoteBrown, Colorway::RangerGreen, Colorway::Black],
            Colorway::Multicam,
            &[("Optactical", 4_399, true), ("SKD Tactical", 4_650, true)],
        ),
    ),
    // --- adapter hardware -----------------------------------------------------
    (
        "axl-avp-jpc",
        (
            6_000,
            &[Colorway::CoyoteBrown, Colorway::Multicam, Colorway::RangerGreen],
            Colorway::CoyoteBrown,
            &[("SKD Tactical", 5_299, true), ("Optactical", 5_650, true)],
        ),
    ),
    (
        "axl-eclipse-velcro-adapter",
        (
            4_500,
            &[Colorway::Multicam, Colorway::CoyoteBrown, Colorway::RangerGreen],
            Colorway::Multicam,
            &[("Optactical", 3_999, true), ("Gritr Sports", 4_200, true)],
        ),
    ),
    (
        "ferro-3ac-tubes-adapter",
        (
            5_500,
            &[Colorway::Multicam, Colorway::CoyoteBrown, Colorway::RangerGreen],
            Colorway::Multicam,
            &[("SKD Tactical", 4_899, true), ("Primary Arms", 5_150, true)],
        ),
    ),
    (
        "ghook-to-qasm-buckle-kit",
        (
            3_500,
            &[Colorway::Multicam, Colorway::CoyoteBrown, Colorway::RangerGreen, Colorway::Black],
            Colorway::Multicam,
            &[("Optactical", 3_099, true), ("Vingtqlc Ops", 3_300, true)],
        ),
    ),
];

/// Typed operational capabilities per component SKU. Adapters and platforms
/// carry none (R3: capability checks never read names or free text).
const COMPONENT_CAPABILITIES: &[(&str, &[CapabilityTag])] = &[
    ("spiritus-mk5-placard", &[CapabilityTag::RifleMagCapacity(3)]),
    ("ferro-dope-flap", &[CapabilityTag::AdminAtakUtility]),
    ("haley-d3crm-placard", &[CapabilityTag::ProneLowProfile]),
    ("esstac-kywi-556-double", &[CapabilityTag::RifleMagCapacity(2)]),
    (
        "spiritus-spud-pouch",
        &[CapabilityTag::CommsRoutingOrPtt, CapabilityTag::HydrationSustainment],
    ),
    ("ferro-roll-1-ifak", &[CapabilityTag::MedicalIfak]),
    ("crye-zip-on-pack-2-0", &[CapabilityTag::HydrationSustainment]),
    (
        "ferro-back-panel-banger",
        &[CapabilityTag::BreachingOrOrdnance, CapabilityTag::HydrationSustainment],
    ),
    ("safariland-ubl-mid-ride", &[CapabilityTag::HolsterRetention]),
    ("bfg-micro-trauma-now", &[CapabilityTag::MedicalIfak]),
    ("agilite-ptt-3-panel", &[CapabilityTag::CommsRoutingOrPtt]),
    ("ferro-concepts-admin-pouch", &[CapabilityTag::AdminAtakUtility]),
];

/// A fully resolved pricing record for one item.
#[derive(Debug, Clone, PartialEq)]
pub struct ItemPricing {
    /// MSRP in US cents.
    pub msrp_cents: u32,
    /// Vendor listings.
    pub offers: Vec<VendorOffer>,
    /// Offered colorways.
    pub allowed_colorways: Vec<Colorway>,
    /// Default colorway.
    pub default_colorway: Colorway,
}

/// Resolve pricing for a platform SKU (`None` when the table has no entry —
/// the normalizer turns that into a loud ingest error).
pub fn platform_pricing(sku: &str) -> Option<ItemPricing> {
    item_pricing(sku, PLATFORM_SPECS)
}

/// Resolve pricing for a component or adapter SKU.
pub fn item_offer_pricing(sku: &str) -> Option<ItemPricing> {
    item_pricing(sku, ITEM_SPECS)
}

/// Resolve capabilities for a component SKU (empty when unknown).
pub fn capabilities(sku: &str) -> Vec<CapabilityTag> {
    COMPONENT_CAPABILITIES
        .iter()
        .find(|(known_sku, _)| *known_sku == sku)
        .map(|(_, tags)| tags.to_vec())
        .unwrap_or_default()
}

/// Table lookup shared by platforms and items.
fn item_pricing(sku: &str, table: &[(&str, ItemSpec)]) -> Option<ItemPricing> {
    let (_, (msrp_cents, colorways, default_colorway, offer_specs)) =
        table.iter().find(|(known_sku, _)| *known_sku == sku)?;
    Some(ItemPricing {
        msrp_cents: *msrp_cents,
        offers: offers(sku, offer_specs),
        allowed_colorways: colorways.to_vec(),
        default_colorway: *default_colorway,
    })
}

/// The six curated role presets. Placements are applied in the fixed order
/// listed; child-slot paths (`"m2.placard_front_pals"`) use the mount ids
/// this same order produces — adapters always consume ids before the
/// components they serve — so every application is byte-identical.
pub fn role_presets() -> Vec<RolePreset> {
    vec![
        RolePreset {
            preset_id: "breacher".into(),
            name: "Breacher".into(),
            role: Some(OperationalRole::Breacher),
            policy: RegulationPolicy::UsArmyAr670_1,
            platform_sku: "ferro-concepts-fcpc-v5".into(),
            default_colorway: Colorway::Multicam,
            placements: vec![
                // G-Hook front placard field takes the MK5 through the
                // G-Hook→QASM buckle kit (adapter m1, placard m2).
                PresetPlacement {
                    slot_id: "front_placard".into(),
                    component_sku: "spiritus-mk5-placard".into(),
                    col: 0,
                    row: 0,
                },
                // Native FerroZip rear panel: breaching charge carriage.
                PresetPlacement {
                    slot_id: "rear_panel".into(),
                    component_sku: "ferro-back-panel-banger".into(),
                    col: 0,
                    row: 0,
                },
                // Three KYWI cells across the placard's front PALS field
                // (the 6x3 grid's row-0/1 band).
                PresetPlacement {
                    slot_id: "m2.placard_front_pals".into(),
                    component_sku: "esstac-kywi-556-double".into(),
                    col: 0,
                    row: 0,
                },
                PresetPlacement {
                    slot_id: "m2.placard_front_pals".into(),
                    component_sku: "esstac-kywi-556-double".into(),
                    col: 2,
                    row: 0,
                },
                PresetPlacement {
                    slot_id: "m2.placard_front_pals".into(),
                    component_sku: "esstac-kywi-556-double".into(),
                    col: 4,
                    row: 0,
                },
                // IFAK on the SwiftClip dangler zone.
                PresetPlacement {
                    slot_id: "dangler_slot".into(),
                    component_sku: "bfg-micro-trauma-now".into(),
                    col: 0,
                    row: 0,
                },
            ],
        },
        RolePreset {
            preset_id: "jtac".into(),
            name: "JTAC".into(),
            role: Some(OperationalRole::Jtac),
            policy: RegulationPolicy::UsArmyAr670_1,
            platform_sku: "crye-precision-jpc-2-0".into(),
            default_colorway: Colorway::Multicam,
            placements: vec![
                // Crye AVS flap takes the MK5 through the AXL AVP (m1/m2).
                PresetPlacement {
                    slot_id: "front_placard".into(),
                    component_sku: "spiritus-mk5-placard".into(),
                    col: 0,
                    row: 0,
                },
                // Admin/ATAK panel + two KYWI cells fill the placard's PALS
                // row; the grid is 6x3 so 2x2 pouches sit in rows 0-1.
                PresetPlacement {
                    slot_id: "m2.placard_front_pals".into(),
                    component_sku: "ferro-concepts-admin-pouch".into(),
                    col: 0,
                    row: 0,
                },
                PresetPlacement {
                    slot_id: "m2.placard_front_pals".into(),
                    component_sku: "esstac-kywi-556-double".into(),
                    col: 2,
                    row: 0,
                },
                PresetPlacement {
                    slot_id: "m2.placard_front_pals".into(),
                    component_sku: "esstac-kywi-556-double".into(),
                    col: 4,
                    row: 0,
                },
                // IFAK on the SwiftClip dangler zone.
                PresetPlacement {
                    slot_id: "dangler_slot".into(),
                    component_sku: "bfg-micro-trauma-now".into(),
                    col: 0,
                    row: 0,
                },
            ],
        },
        RolePreset {
            preset_id: "drone-operator".into(),
            name: "Drone Operator".into(),
            role: Some(OperationalRole::DroneOperator),
            policy: RegulationPolicy::UsArmyAr670_1,
            platform_sku: "spiritus-systems-lv-119-overt".into(),
            default_colorway: Colorway::Multicam,
            placements: vec![
                // Native SwiftClip placard field: no adapter needed (m1).
                PresetPlacement {
                    slot_id: "front_placard".into(),
                    component_sku: "spiritus-mk5-placard".into(),
                    col: 0,
                    row: 0,
                },
                PresetPlacement {
                    slot_id: "m1.placard_front_pals".into(),
                    component_sku: "ferro-concepts-admin-pouch".into(),
                    col: 0,
                    row: 0,
                },
                PresetPlacement {
                    slot_id: "m1.placard_front_pals".into(),
                    component_sku: "esstac-kywi-556-double".into(),
                    col: 2,
                    row: 0,
                },
                PresetPlacement {
                    slot_id: "m1.placard_front_pals".into(),
                    component_sku: "esstac-kywi-556-double".into(),
                    col: 4,
                    row: 0,
                },
                // IFAK on the SwiftClip dangler zone (native fit).
                PresetPlacement {
                    slot_id: "dangler_slot".into(),
                    component_sku: "bfg-micro-trauma-now".into(),
                    col: 0,
                    row: 0,
                },
            ],
        },
        RolePreset {
            preset_id: "sniper-dm".into(),
            name: "Sniper / DM".into(),
            role: Some(OperationalRole::SniperDm),
            policy: RegulationPolicy::UsArmyAr670_1,
            platform_sku: "t-rex-arms-ac1".into(),
            default_colorway: Colorway::Multicam,
            placements: vec![
                // Chest rig main cell (6x3 PALS): three KYWI cells, keeping
                // the front slick for prone work.
                PresetPlacement {
                    slot_id: "rig_main_cell".into(),
                    component_sku: "esstac-kywi-556-double".into(),
                    col: 0,
                    row: 0,
                },
                PresetPlacement {
                    slot_id: "rig_main_cell".into(),
                    component_sku: "esstac-kywi-556-double".into(),
                    col: 2,
                    row: 0,
                },
                PresetPlacement {
                    slot_id: "rig_main_cell".into(),
                    component_sku: "esstac-kywi-556-double".into(),
                    col: 4,
                    row: 0,
                },
                // IFAK anchors onto the PALS wing (non-grid mount); the AC1's
                // dangler zone is G-Hook, which the PALS IFAK cannot use.
                PresetPlacement {
                    slot_id: "wing_left".into(),
                    component_sku: "ferro-roll-1-ifak".into(),
                    col: 0,
                    row: 0,
                },
            ],
        },
        RolePreset {
            preset_id: "recce".into(),
            name: "Recce".into(),
            role: Some(OperationalRole::Recce),
            policy: RegulationPolicy::UsArmyAr670_1,
            platform_sku: "crye-precision-jpc-2-0".into(),
            default_colorway: Colorway::Multicam,
            placements: vec![
                PresetPlacement {
                    slot_id: "front_placard".into(),
                    component_sku: "spiritus-mk5-placard".into(),
                    col: 0,
                    row: 0,
                },
                // Native Crye #10 zip: hydration/sustainment pack.
                PresetPlacement {
                    slot_id: "rear_panel".into(),
                    component_sku: "crye-zip-on-pack-2-0".into(),
                    col: 0,
                    row: 0,
                },
                PresetPlacement {
                    slot_id: "m2.placard_front_pals".into(),
                    component_sku: "ferro-concepts-admin-pouch".into(),
                    col: 0,
                    row: 0,
                },
                PresetPlacement {
                    slot_id: "m2.placard_front_pals".into(),
                    component_sku: "esstac-kywi-556-double".into(),
                    col: 2,
                    row: 0,
                },
                PresetPlacement {
                    slot_id: "m2.placard_front_pals".into(),
                    component_sku: "esstac-kywi-556-double".into(),
                    col: 4,
                    row: 0,
                },
                // IFAK on the SwiftClip dangler zone (native fit).
                PresetPlacement {
                    slot_id: "dangler_slot".into(),
                    component_sku: "bfg-micro-trauma-now".into(),
                    col: 0,
                    row: 0,
                },
            ],
        },
        RolePreset {
            preset_id: "le-patrol".into(),
            name: "LE Patrol".into(),
            // Patrol is a regulation profile, not an operational role
            // profile: the preset validates against the policy alone.
            role: None,
            policy: RegulationPolicy::LawEnforcementPatrol,
            platform_sku: "aws-smu-belt".into(),
            default_colorway: Colorway::Black,
            placements: vec![
                // Holster drop zone (2x3): UBL mid-ride mounts exactly.
                PresetPlacement {
                    slot_id: "holster_drop_zone".into(),
                    component_sku: "safariland-ubl-mid-ride".into(),
                    col: 0,
                    row: 0,
                },
                PresetPlacement {
                    slot_id: "belt_outer_pals".into(),
                    component_sku: "ferro-roll-1-ifak".into(),
                    col: 0,
                    row: 0,
                },
                PresetPlacement {
                    slot_id: "belt_outer_pals".into(),
                    component_sku: "esstac-kywi-556-double".into(),
                    col: 2,
                    row: 0,
                },
                PresetPlacement {
                    slot_id: "belt_outer_pals".into(),
                    component_sku: "agilite-ptt-3-panel".into(),
                    col: 4,
                    row: 0,
                },
            ],
        },
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_platform_spec_carries_two_or_more_offers() {
        for (sku, (msrp, _, _, offers)) in PLATFORM_SPECS {
            assert!(*msrp > 0, "{sku} needs a nonzero MSRP");
            assert!(offers.len() >= 2, "{sku} needs at least two vendor offers");
        }
    }

    #[test]
    fn default_colorway_is_always_offered() {
        for (sku, (_, colorways, default, _)) in PLATFORM_SPECS.iter().chain(ITEM_SPECS.iter()) {
            assert!(
                colorways.contains(default),
                "{sku} default colorway must be among the offered colorways"
            );
        }
    }

    #[test]
    fn every_seeded_component_and_adapter_has_pricing() {
        let (adapters, components) = crate::normalizer::seed_core_adapters_and_components();
        for adapter in &adapters {
            assert!(
                item_offer_pricing(&adapter.sku).is_some(),
                "{} needs a pricing_seed entry",
                adapter.sku
            );
        }
        for component in &components {
            assert!(
                item_offer_pricing(&component.sku).is_some(),
                "{} needs a pricing_seed entry",
                component.sku
            );
            assert!(
                !component.capabilities.is_empty(),
                "{} needs at least one capability tag",
                component.sku
            );
        }
    }
}
