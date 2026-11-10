//! `data-ingest` CLI: CSV normalization → `catalog_seed.json` + `seed.sql`.

use clap::Parser;
use data_ingest::normalizer::normalize_catalog;
use data_ingest::sample_csvs::write_sample_csvs;
use data_ingest::sql_gen::generate_seed_sql;
use std::path::PathBuf;

/// Ingest the tactical gear Google Sheet CSVs and emit catalog seed files.
#[derive(Debug, Parser)]
#[command(name = "data-ingest", version, about)]
struct Cli {
    /// Directory containing the five raw CSV exports.
    #[arg(long, value_name = "PATH", default_value = ".")]
    input_dir: PathBuf,
    /// Directory to write `catalog_seed.json` and `seed.sql` into.
    #[arg(long, value_name = "PATH", default_value = ".")]
    output_dir: PathBuf,
    /// Write the five sample CSVs (Google Sheet records) into `--input-dir`
    /// first, so the pipeline can be tested self-contained.
    #[arg(long)]
    generate_sample_csvs: bool,
}

fn main() {
    let cli = Cli::parse();
    match run(&cli) {
        Ok(summary) => {
            println!("{summary}");
        }
        Err(err) => {
            eprintln!("error: {err}");
            std::process::exit(1);
        }
    }
}

fn run(cli: &Cli) -> Result<String, data_ingest::IngestError> {
    if cli.generate_sample_csvs {
        write_sample_csvs(&cli.input_dir)?;
        eprintln!(
            "wrote 5 sample CSVs to {}",
            cli.input_dir.display()
        );
    }

    let report = normalize_catalog(&cli.input_dir)?;
    let catalog = &report.catalog;

    std::fs::create_dir_all(&cli.output_dir)?;
    let json_path = cli.output_dir.join("catalog_seed.json");
    let sql_path = cli.output_dir.join("seed.sql");

    std::fs::write(&json_path, serde_json::to_string_pretty(catalog)?)?;
    std::fs::write(&sql_path, generate_seed_sql(catalog))?;

    let rows: usize = report.rows_ingested.iter().map(|(_, n)| n).sum();
    Ok(format!(
        "ingested {} rows -> {} platforms, {} components, {} adapter bridges\n  {}\n  {}",
        rows,
        catalog.platforms.len(),
        catalog.components.len(),
        catalog.adapter_bridges.len(),
        json_path.display(),
        sql_path.display()
    ))
}
