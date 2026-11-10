//! `api-server` binary: bootstraps the catalog and serves the REST API.
//!
//! Configuration (environment variables):
//! - `CATALOG_SEED` — optional path to a pre-generated `catalog_seed.json`;
//!   when absent the catalog is synthesized through the data-ingest
//!   normalizer.
//! - `LISTEN_ADDR` — bind address, default `0.0.0.0:8080`.
//! - `DATABASE_URL` — SQLite connection string for the persistence layer,
//!   default `sqlite://tactical_gear.db?mode=rwc`. The database is migrated
//!   and self-seeded with the bootstrapped catalog on first boot.

use api_server::{build_router, load_catalog, AppState};
use store::Store;

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

    let db_url = std::env::var("DATABASE_URL")
        .unwrap_or_else(|_| "sqlite://tactical_gear.db?mode=rwc".to_owned());
    let store = match Store::connect_and_migrate(&db_url).await {
        Ok(store) => store,
        Err(error) => {
            eprintln!("error connecting {db_url}: {error}");
            std::process::exit(1);
        }
    };
    match store.ensure_seeded(&catalog).await {
        Ok(version) => println!("catalog revision {version} active in {db_url}"),
        Err(error) => {
            eprintln!("error seeding {db_url}: {error}");
            std::process::exit(1);
        }
    }

    let app = build_router(AppState::with_store(catalog, store));
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
