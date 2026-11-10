//! `api-server` binary: bootstraps the catalog and serves the REST API.
//!
//! Configuration (environment variables):
//! - `CATALOG_SEED` — optional path to a pre-generated `catalog_seed.json`;
//!   when absent the catalog is synthesized through the data-ingest
//!   normalizer.
//! - `LISTEN_ADDR` — bind address, default `0.0.0.0:8080`.

use api_server::{build_router, load_catalog, AppState};

#[tokio::main]
async fn main() {
    let seed_path = std::env::var("CATALOG_SEED")
        .ok()
        .map(std::path::PathBuf::from);
    let catalog = match load_catalog(seed_path.as_deref()) {
        Ok(catalog) => catalog,
        Err(error) => {
            eprintln!("error: {error}");
            std::process::exit(1);
        }
    };
    let addr = std::env::var("LISTEN_ADDR").unwrap_or_else(|_| "0.0.0.0:8080".to_owned());

    let app = build_router(AppState::new(catalog));
    let listener = match tokio::net::TcpListener::bind(&addr).await {
        Ok(listener) => listener,
        Err(error) => {
            eprintln!("error binding {addr}: {error}");
            std::process::exit(1);
        }
    };
    println!("api-server listening on http://{addr}");
    if let Err(error) = axum::serve(listener, app).await {
        eprintln!("error: {error}");
        std::process::exit(1);
    }
}
