# AGENTS.md

## Navigation

- Domain glossary: `GLOSSARY.md`
- Architectural Decision Records: `docs/adr/`
- Coding standards & invariants: `CODING_STANDARDS.md`
- Rust compatibility engine: `crates/core-engine/`
- CSV ingest & seed generator: `crates/data-ingest/`
- SQLite persistence store: `crates/store/`
- Axum REST API: `crates/api-server/`
- WebAssembly bindings: `crates/wasm-engine/`
- React 2D visual builder: `frontend/`

## Agent skills

### Issue tracker

Issues and specs are tracked as local markdown files under `.scratch/<feature-slug>/`. See `docs/agents/issue-tracker.md`.

### Triage labels

Default five-role triage vocabulary (`needs-triage`, `needs-info`, `ready-for-agent`, `ready-for-human`, `wontfix`). See `docs/agents/triage-labels.md`.

### Domain docs

Single-context layout (`GLOSSARY.md` and `docs/adr/` at the repo root). See `docs/agents/domain.md`.
