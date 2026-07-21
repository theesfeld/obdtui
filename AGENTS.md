# obdtui — project facts

Repo overlay only. Global constitution: `~/.config/agents/AGENTS.md`.

## Layout

- `crates/obd-io` — library (transport, J1979, capture, profiles)
- `crates/obd-sim` — fixtures / replay helpers
- `crates/obd-tui` — `obdtui` binary
- `profiles/` — vehicle YAML
- `schemas/` — capture JSON Schema
- `site/` — GitHub Pages

## Commands

```sh
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace --all-features
cargo run -p obd-tui -- --replay fixtures/sample-session
```

## Safety

- Default product mode: read-only
- No module programming in phase 0
- Clear DTC only behind `--allow-writes`
