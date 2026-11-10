//! Raw serde models mirroring the exact Google Sheet CSV column headers.
//!
//! Every field is a plain `String`: these structs capture what the humans
//! typed. Deterministic normalization into `core_engine` enums happens in
//! [`crate::normalizer`], which fails loudly (`IngestError::UnmappedInterface`)
//! rather than guessing.

use serde::Deserialize;

/// Row of `plate_carriers.csv`.
#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct RawPlateCarrierRow {
    /// `Platform Name`
    #[serde(rename = "Platform Name")]
    pub platform_name: String,
    /// `Brand`
    #[serde(rename = "Brand")]
    pub brand: String,
    /// `Plate Cut / Sizing`
    #[serde(rename = "Plate Cut / Sizing")]
    pub plate_cut_sizing: String,
    /// `Front Placard Interface`
    #[serde(rename = "Front Placard Interface")]
    pub front_placard_interface: String,
    /// `Cummerbund Interface`
    #[serde(rename = "Cummerbund Interface")]
    pub cummerbund_interface: String,
    /// `Back Panel / Zipper Standard`
    #[serde(rename = "Back Panel / Zipper Standard")]
    pub back_panel_zipper_standard: String,
    /// `Comms / Hydration Routing`
    #[serde(rename = "Comms / Hydration Routing")]
    pub comms_hydration_routing: String,
    /// `Primary Role / Weight Class`
    #[serde(rename = "Primary Role / Weight Class")]
    pub primary_role_weight_class: String,
}

/// Row of `belts.csv`.
#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct RawBeltRow {
    /// `Belt Name`
    #[serde(rename = "Belt Name")]
    pub belt_name: String,
    /// `Brand`
    #[serde(rename = "Brand")]
    pub brand: String,
    /// `Belt System Type`
    #[serde(rename = "Belt System Type")]
    pub belt_system_type: String,
    /// `Inner/Outer Velcro Spec`
    #[serde(rename = "Inner/Outer Velcro Spec")]
    pub inner_outer_velcro_spec: String,
    /// `Width & Stiffener Material`
    #[serde(rename = "Width & Stiffener Material")]
    pub width_stiffener_material: String,
    /// `Buckle Type`
    #[serde(rename = "Buckle Type")]
    pub buckle_type: String,
    /// `MOLLE / PALS Mounting Spec`
    #[serde(rename = "MOLLE / PALS Mounting Spec")]
    pub molle_pals_mounting_spec: String,
    /// `Holster Mounting Compatibility`
    #[serde(rename = "Holster Mounting Compatibility")]
    pub holster_mounting_compatibility: String,
}

/// Row of `chest_rigs.csv`.
#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct RawChestRigRow {
    /// `Rig Name`
    #[serde(rename = "Rig Name")]
    pub rig_name: String,
    /// `Brand`
    #[serde(rename = "Brand")]
    pub brand: String,
    /// `Category`
    #[serde(rename = "Category")]
    pub category: String,
    /// `Harness & Buckle Interface`
    #[serde(rename = "Harness & Buckle Interface")]
    pub harness_buckle_interface: String,
    /// `Internal Pocket / Insert Layout`
    #[serde(rename = "Internal Pocket / Insert Layout")]
    pub internal_pocket_insert_layout: String,
    /// `Native Magazine Capacity`
    #[serde(rename = "Native Magazine Capacity")]
    pub native_magazine_capacity: String,
    /// `Wing / Expander Compatibility`
    #[serde(rename = "Wing / Expander Compatibility")]
    pub wing_expander_compatibility: String,
    /// `Sub-Load / Dangler Interface`
    #[serde(rename = "Sub-Load / Dangler Interface")]
    pub sub_load_dangler_interface: String,
}

/// Row of `lbe.csv`.
#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct RawLbeRow {
    /// `LBE System Name`
    #[serde(rename = "LBE System Name")]
    pub lbe_system_name: String,
    /// `Brand`
    #[serde(rename = "Brand")]
    pub brand: String,
    /// `Harness / Yoke Style`
    #[serde(rename = "Harness / Yoke Style")]
    pub harness_yoke_style: String,
    /// `Hip Belt / Pad Structure`
    #[serde(rename = "Hip Belt / Pad Structure")]
    pub hip_belt_pad_structure: String,
    /// `Integrated vs. MOLLE Pouches`
    #[serde(rename = "Integrated vs. MOLLE Pouches")]
    pub integrated_vs_molle_pouches: String,
    /// `Sustainment Capacity`
    #[serde(rename = "Sustainment Capacity")]
    pub sustainment_capacity: String,
    /// `Buttpack / Field Pack Integration`
    #[serde(rename = "Buttpack / Field Pack Integration")]
    pub buttpack_field_pack_integration: String,
    /// `Armor / Ruck Compatibility`
    #[serde(rename = "Armor / Ruck Compatibility")]
    pub armor_ruck_compatibility: String,
}

/// Row of `compatibility_schema.csv`.
#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct RawSchemaRuleRow {
    /// `Interface Category`
    #[serde(rename = "Interface Category")]
    pub interface_category: String,
    /// `Standard / Specification Name`
    #[serde(rename = "Standard / Specification Name")]
    pub standard_specification_name: String,
    /// `Dimensional / Technical Spec`
    #[serde(rename = "Dimensional / Technical Spec")]
    pub dimensional_technical_spec: String,
    /// `Native Compatible Brands/Platforms`
    #[serde(rename = "Native Compatible Brands/Platforms")]
    pub native_compatible_brands_platforms: String,
    /// `Conflicting / Incompatible Standards`
    #[serde(rename = "Conflicting / Incompatible Standards")]
    pub conflicting_incompatible_standards: String,
    /// `Required Bridge / Adapter Solution`
    #[serde(rename = "Required Bridge / Adapter Solution")]
    pub required_bridge_adapter_solution: String,
    /// `Notes`
    #[serde(rename = "Notes")]
    pub notes: String,
}
