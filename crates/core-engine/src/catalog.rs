//! The in-memory product catalog consumed by the loadout builder and the API
//! layer.
//!
//! This is the canonical shared catalog type: `data-ingest` normalizes CSVs
//! into it (and re-exports it), seeds serialize to it, and `api-server` serves
//! and evaluates against it.

use crate::graph::{AdapterBridge, ComponentSpec, PlatformSpec};
use serde::{Deserialize, Serialize};

/// The complete normalized catalog: platforms, starter components, and
/// aftermarket adapter bridges.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Catalog {
    /// Normalized platform specs (plate carriers, belts, chest rigs, LBE).
    pub platforms: Vec<PlatformSpec>,
    /// Starter child components.
    pub components: Vec<ComponentSpec>,
    /// Aftermarket adapter bridges.
    pub adapter_bridges: Vec<AdapterBridge>,
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
}
