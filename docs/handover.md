# Handover

**Step 1 of `docs/plan.md` is done (2026-11-24). Next: Step 2, the dashboard server.**

## What exists
- `SPEC.md`: widget contract v2, with `schema/catalogue.schema.json` and `schema/data.schema.json`.
- Demo widget app: `cargo run -- --demo` → `http://127.0.0.1:4701/widgets`. Three made-up sources (`server`, `weather`, `spending`), every field type, params, thresholds both ways, 12 presets.
- `src/contract.rs` already has what the dashboard needs from the contract: the types, `Catalogue::problems()`, `Source::data_problems()`, `Field::check_entry()`, `Field::status_of()`.
- 57 tests, `cargo clippy --all-targets -- -D warnings` clean.

## Step 2 starts with
- **Third-party gate first.** The dashboard needs an HTTP client (to call apps) and a config file parser. Name the crates, publisher, maturity, how they update and what they reach, then wait for the owner's go. Nothing is added before that.
- **Decide and say why:** layout storage (JSON file or SQLite).
- **`--demo` should become** "dashboard + demo app together, zero config". Today it only starts the demo app.
- **Token path:** the demo app has no token option yet. Add one when the dashboard's bearer-token code needs a test.
- One bad field should not take down a whole source: use `Field::check_entry()` per field, not all-or-nothing.

## Standing rules
- Repo is public: read the "This repo is PUBLIC" section of `CLAUDE.md` before writing anything.
- Every crate / JS library goes through the third-party gate in `CLAUDE.md` first.
