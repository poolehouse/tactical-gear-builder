//! `data-ingest` — CSV normalization, seed generator, and validation CLI.
//!
//! Reads the five raw Google Sheet exports (`plate_carriers.csv`, `belts.csv`,
//! `chest_rigs.csv`, `lbe.csv`, `compatibility_schema.csv`), deterministically
//! normalizes human-readable specs into strict `core-engine` enums, synthesizes
//! starter child components and adapter bridges from the compatibility schema,
//! and emits `catalog_seed.json` + `seed.sql`.
//!
//! Zero silent fallbacks: any interface string that cannot be deterministically
//! mapped produces [`IngestError::UnmappedInterface`] with the row, column, and
//! offending raw value.

#![forbid(unsafe_code)]
#![warn(missing_docs)]

pub mod csv_models;
pub mod normalizer;
pub mod sample_csvs;
pub mod sql_gen;

use serde::Deserialize;
use std::path::PathBuf;
use thiserror::Error;

/// Shared catalog type, defined in `core-engine` so the loadout builder and
/// API server can consume ingest output without a reverse dependency.
pub use core_engine::catalog::Catalog;

/// Every failure mode of the ingest pipeline.
#[derive(Debug, Error)]
pub enum IngestError {
    /// A CSV cell could not be deterministically mapped to a
    /// `core_engine::InterfaceStandard`.
    #[error("unmapped interface at CSV row {row}, column `{column}`: {raw_value:?}")]
    UnmappedInterface {
        /// 1-based CSV data row number.
        row: usize,
        /// CSV column header the bad value came from.
        column: String,
        /// The offending raw cell text.
        raw_value: String,
    },
    /// A plate-cut cell could not be mapped to `core_engine::PlateCut`.
    #[error("unmapped plate cut at CSV row {row}, column `{column}`: {raw_value:?}")]
    UnmappedPlateCut {
        /// 1-based CSV data row number.
        row: usize,
        /// CSV column header the bad value came from.
        column: String,
        /// The offending raw cell text.
        raw_value: String,
    },
    /// A required CSV file is missing from the input directory.
    #[error("missing required input file: {0}")]
    MissingInput(PathBuf),
    /// Underlying filesystem failure.
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
    /// CSV parse failure with row context.
    #[error("csv parse error in {file}: {source}")]
    Csv {
        /// File being parsed.
        file: String,
        /// Underlying csv crate error.
        #[source]
        source: csv::Error,
    },
    /// JSON serialization failure.
    #[error("json error: {0}")]
    Json(#[from] serde_json::Error),
}

/// Convert bare csv crate errors (e.g. from the sample writer) into the
/// contextual [`IngestError::Csv`] variant with a generic file label.
impl From<csv::Error> for IngestError {
    fn from(source: csv::Error) -> Self {
        IngestError::Csv {
            file: "<csv>".to_owned(),
            source,
        }
    }
}

/// A parsed and normalized result: the catalog plus provenance about which
/// source files and rows produced each platform.
#[derive(Debug, Clone, PartialEq)]
pub struct IngestReport {
    /// The normalized catalog.
    pub catalog: Catalog,
    /// Number of data rows ingested, per source file.
    pub rows_ingested: Vec<(String, usize)>,
}

/// Read and parse a typed CSV file from `dir`, converting csv crate errors
/// into contextual [`IngestError::Csv`] failures.
pub fn read_csv<T: for<'de> Deserialize<'de>>(
    dir: &std::path::Path,
    file: &str,
) -> Result<Vec<T>, IngestError> {
    let path = dir.join(file);
    if !path.is_file() {
        return Err(IngestError::MissingInput(path));
    }
    let mut reader = csv::Reader::from_path(&path)?;
    let rows: Vec<Result<T, csv::Error>> = reader.deserialize().collect();
    let rows = rows
        .into_iter()
        .collect::<Result<Vec<T>, csv::Error>>()
        .map_err(|source| IngestError::Csv {
            file: file.to_owned(),
            source,
        })?;
    Ok(rows)
}
