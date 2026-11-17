# Coding Standards

## Standing Architectural Rules (R1–R8)

- **R1 (Pure Core Engine)**: `crates/core-engine` must remain 100% pure Rust — no I/O, no filesystem, no network, no async runtime, and `#![forbid(unsafe_code)]`. It must compile cleanly to both native targets and `wasm32-unknown-unknown`.
- **R2 (Fail-Closed Ingest & Persistence)**: Never silently coerce, default, or skip unmappable CSV rows, missing pricing entries, or corrupted database records. Every invalid input must return a structured, row/column- or field-specific error.
- **R3 (Typed Enums Over Free Text)**: Compatibility, kit sections, capabilities, regulations, and roles are resolved exclusively through typed enums (`InterfaceStandard`, `InterfaceFamily`, `KitSection`, `ComponentKind`, `CapabilityTag`), never by matching on product names, SKUs, or free-form strings.
- **R4 (Semantic vs. Transport Errors)**: Placement collisions, incompatibility, and regulation violations are valid semantic outcomes returned under HTTP 200 (with `valid: false` or `compliant: false`), never HTTP 4xx/5xx transport errors.
- **R5 (Byte-Deterministic Seeds & Money)**: All monetary values are integer US cents (`u32`), never floats. Re-running `data-ingest` on the same CSV inputs must produce byte-identical `catalog_seed.json` and `seed.sql` outputs.
- **R6 (Clippy & Warning Cleanliness)**: Every crate must build with `#![forbid(unsafe_code)]`, `#![warn(missing_docs)]`, and pass `cargo clippy --workspace --all-targets -- -D warnings` with zero warnings.
- **R7 (1:1 TypeScript DTO Mirror)**: `frontend/src/types.ts` must mirror the serde wire shapes of `core-engine` / `wasm-engine` 1:1.
- **R8 (Zero Network Calls in Browser Demo)**: After initial page + WASM asset load, the browser demo must perform zero network requests; all compatibility, occupancy, pricing, and compliance computation runs client-side in WebAssembly.
