# Changelog

All notable changes to this project are documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project follows [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added

- **`obd-mfd`:** windowed vector HUD (egui) — real arcs/needles, phosphor green +
  red/amber warnings (not terminal cell art)
- **F1 Full Mode 01 capture:** enumerate all supported PID blocks; poll every
  supported PID while capture is on; priority + bulk scheduler
- **F2 Vector gauges:** single instrument + multi-gauge row (arc + tape)
- **F3 J1979 depth:** freeze frame Mode 02, permanent DTCs 0A, CALID 09 04,
  monitor status hook
- **F4 Dual-bus / modules:** STN HS/MS select (`ATSP6` / `STP53`), Ford module
  read probes
- **F5 Write audit:** clear-DTC still gated; audit log on writes
- **F6 MFD shell:** vector HUD layout tab (bind for future embedded front-end)
- Expanded SAE PID decode catalog (load, MAP, MAF, oil temp, fuel rate, …)
- Phase 0 core: transport, capture package, profiles, BT SPP, TUI

[Unreleased]: https://github.com/theesfeld/obdtui/compare/v0.1.0-dev.1...HEAD
