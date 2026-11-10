# tactical-gear-builder

A compatibility engine and 2D loadout builder for tactical platforms (plate
carriers, belts, chest rigs) — the kind of tooling that would back a
configurator-style storefront for mission-specific gear.

**Live demo:** <https://poolehouse.github.io/tactical-gear-builder/>

The entire compatibility engine is Rust. It runs in three places — as an
Axum REST service, as a native CLI ingest pipeline, and compiled to
WebAssembly inside the browser demo — with zero divergence, because the same
`core-engine` crate powers all three.

## What it does

- **Typed interface model.** Mount systems (PALS/MOLLE webbing, Crye AVS
  flaps, SwiftClip, G-hooks, belt pass-throughs…) are an enum, not free text.
  Compatibility is resolved through interface families and adapter bridges —
  never string matching.
- **PALS occupancy.** MOLLE real estate is a grid. Mounting a 2×2 pouch marks
  four cells occupied; overlapping placements are rejected with the exact
  colliding cells reported.
- **Adapter auto-injection.** A SwiftClip placard on a Crye AVS flap is
  classified `requires_adapter`, and the required bridge hardware (e.g. the
  AXL AVP) is automatically added to the bill of materials — and removed
  again when the component is unequipped.
- **Deterministic 2D placement.** Every mount carries a percentage-space
  anchor (`AnchorBox2D`) with explicit z-layers (platform → adapter →
  component → child component), composed hierarchically so child slots ride
  their host. The demo renders exactly these values.
- **Fail-closed ingest.** Catalog CSVs with unknown interfaces, missing
  anchors, or malformed rows are rejected with row/column-precise errors —
  never silently coerced or defaulted.
- **Vendor pricing & BOM rollup.** Every platform, component, and adapter
  carries an integer-cents MSRP plus competitive vendor listings. The BOM
  totals MSRP, the optimized build cost (best in-stock offer per line,
  lowest price wins, ties broken by vendor name), and total savings — exact
  integer math, no floats.
- **Regulation & role validation.** Military/LE regulation policies
  (AR 670-1, USMC MARADMIN, LE patrol/SWAT) and operational role profiles
  (breacher, JTAC, drone operator, sniper/DM, recce) evaluate a build against
  typed capability tags — never product names. Violations block, advisories
  don't; every finding names mountable suggested SKUs that close the gap.
- **Role presets.** Six curated starter loadouts instantiate through the
  normal engine validation path (adapter auto-injection, PALS collisions,
  pricing) and validate compliant under their own declared policy and role.

## Workspace layout

| Crate | Role |
| --- | --- |
| `crates/core-engine` | Pure compatibility domain: catalog types, interface families, adapter graph, PALS occupancy, `LoadoutDraft` state machine, 2D anchor math. No I/O, no async — compiles anywhere Rust does. |
| `crates/data-ingest` | CSV → canonical catalog pipeline (fail-closed), sample-data generator, and SQL seed writer. |
| `crates/api-server` | Axum REST API over the engine: catalog browsing, per-slot compatibility classification, loadout evaluation. |
| `crates/wasm-engine` | Thin `wasm-bindgen` wrapper exporting the engine (plus embedded catalog seed) to the browser. |
| `frontend/` | React + Vite 2D visual builder — the GitHub Pages demo. |
| `seeds/` | The canonical, byte-deterministic catalog seed (`catalog_seed.json` + `seed.sql`), embedded into the wasm build. |

## REST API sketch

```
GET  /health
GET  /api/v1/catalog/platforms
GET  /api/v1/catalog/platforms/{sku}
GET  /api/v1/catalog/platforms/{sku}/compatible-components?slot_id=
GET  /api/v1/catalog/presets
POST /api/v1/loadouts/evaluate
POST /api/v1/loadouts/from-preset/{preset_id}
```

`compatible-components` classifies every catalog component against a slot as
`native_fit`, `requires_adapter` (with the required bridge SKUs), or
`incompatible` (with a machine-readable reason). `evaluate` runs a full
loadout spec through the engine and returns z-ordered render layers, the
priced bill of materials with totals, and validity with errors; it also
accepts an optional `colorway`, `regulation_policy`, and `operational_role`,
in which case the response carries a compliance report (violations and
advisories are semantic results under HTTP 200, never transport errors).
`from-preset` instantiates a curated role preset and evaluates it under its
own declared context.

## The browser demo

The demo is fully client-side: `wasm-engine` compiles `core-engine` with the
catalog seed baked in at build time, so after the initial page load it makes
**zero network requests** — every compatibility decision, PALS collision, and
adapter auto-injection is computed in your browser by the same code the API
server runs.

Try this flow:

1. Pick **JPC 2.0** (a Crye plate carrier).
2. With `front_placard` selected, mount the **MK5 placard** — watch it get
   classified as adapter-bridged, and the AXL AVP appears in the BOM.
3. Select the placard's child slot `m2.placard_front_pals` and auto-place a
   **KYWI pouch**. Try a second one on the same cells to see collision
   rejection.
4. Select a mount on the canvas and unequip it — child mounts and the
   adapter cascade away.
5. In the context bar, set the colorway to **Black** and the regulation to
   **US Army AR 670-1** — the compliance panel flags the disallowed colorway,
   missing IFAK, and insufficient magazine carriage, each with clickable
   suggested-SKU chips that place the fix for you.
6. Load the **JTAC** role preset — five placements apply through the same
   engine validation path, the compliance context seeds from the preset's
   own declaration (multicam, AR 670-1, JTAC role), and the priced BOM
   includes the auto-injected adapter with per-line vendor offers and
   totals.

## Development

Requirements: Rust (stable), [wasm-pack], Node 18+.

```sh
# tests + lints across the workspace
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings

# run the REST API (serves the seed catalog)
cargo run -p api-server

# regenerate the canonical seed from sample data
cargo run -p data-ingest -- --generate-sample-csvs --input-dir tmp --output-dir seeds

# build the demo
cd frontend
npm install
npm run build:wasm   # wasm-pack build of crates/wasm-engine
npm run build        # tsc + vite build -> frontend/dist
npm run dev          # local dev server
```

Deployment is automatic: a GitHub Actions workflow builds the wasm engine and
the frontend on every push to `main` and publishes `frontend/dist` to GitHub
Pages.

[wasm-pack]: https://rustwasm.github.io/wasm-pack/

## License

MIT — see [LICENSE](LICENSE).
