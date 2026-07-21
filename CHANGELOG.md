# Changelog

All notable changes to this project are documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project follows [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added

- Phase 0 core: `obd-io` transport trait, ELM/STN serial path, J1979 decode
- Capture session package (meta, frames, signals, vehicle snapshot)
- Vehicle profile loader (generic J1979 + Ford stub)
- `obd-sim` replay transport for offline use and CI
- `obdtui` read-only TUI: live PIDs, DTCs, raw log, capture start/stop
- Dual-bus ready fields (`hs` / `ms` / `unknown` bus tags)
- **Dual-path connect:** USB serial and Bluetooth classic SPP (RFCOMM)
  - `connect()` with multi-baud try, `--prefer`, `--bt-mac`, richer `--list-ports`
  - Link kind on adapter capabilities and captures

[Unreleased]: https://github.com/theesfeld/obdtui/compare/v0.1.0-dev.1...HEAD
