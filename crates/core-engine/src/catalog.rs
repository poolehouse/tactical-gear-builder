//! The in-memory product catalog consumed by the loadout builder and the API
//! layer.
//!
//! This is the canonical shared catalog type: `data-ingest` normalizes CSVs
//! into it (and re-exports it), seeds serialize to it, and `api-server` serves
//! and evaluates against it.

use crate::graph::{AdapterBridge, ComponentSpec, PlatformSpec};
use crate::regulation::RolePreset;
use serde::{Deserialize, Serialize};

/// The complete normalized catalog: platforms, starter components, aftermarket
/// adapter bridges, and curated role presets.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Catalog {
    /// Normalized platform specs (plate carriers, belts, chest rigs, LBE).
    pub platforms: Vec<PlatformSpec>,
    /// Starter child components.
    pub components: Vec<ComponentSpec>,
    /// Aftermarket adapter bridges.
    pub adapter_bridges: Vec<AdapterBridge>,
    /// Curated one-click role presets. Defaulted so pre-preset seeds still
    /// deserialize (an empty preset list is honest data, not a guess).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub role_presets: Vec<RolePreset>,
}

impl Catalog {
    /// Total number of records.
    pub fn len(&self) -> usize {
        self.platforms.len() + self.components.len() + self.adapter_bridges.len()
    }

    /// Whether the catalog holds nothing.
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Look up a platform by SKU slug.
    pub fn platform(&self, sku: &str) -> Option<&PlatformSpec> {
        self.platforms.iter().find(|platform| platform.sku == sku)
    }

    /// Look up a component by SKU slug.
    pub fn component(&self, sku: &str) -> Option<&ComponentSpec> {
        self.components.iter().find(|component| component.sku == sku)
    }

    /// Look up an adapter bridge by SKU slug.
    pub fn adapter(&self, sku: &str) -> Option<&AdapterBridge> {
        self.adapter_bridges.iter().find(|adapter| adapter.sku == sku)
    }

    /// Look up a role preset by preset id.
    pub fn preset(&self, preset_id: &str) -> Option<&RolePreset> {
        self.role_presets
            .iter()
            .find(|preset| preset.preset_id == preset_id)
    }
}
