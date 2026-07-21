# obdtui

Linux OBD-II diagnostic TUI and reusable vehicle capture library.

<!-- agents:status:begin -->
> **Status:** Phase 0 bootstrap · Issue: [#1](https://github.com/theesfeld/obdtui/issues/1) · Version: `0.1.0-dev.1` · License: MIT  
> Default mode is **read-only**. Bluetooth classic SPP uses RFCOMM serial. This is not a full FORScan replacement. 0.x may change interfaces.
<!-- agents:status:end -->

## What it does

- Connects to a USB OBD-II adapter (ELM327 / STN serial)
- Shows live Mode 01 PIDs, VIN, and diagnostic trouble codes
- Saves **capture sessions** (raw frames + decoded signals) for other projects
- Replays captures offline for tests and demos without a vehicle
- Uses **vehicle profiles** as YAML data (generic J1979 + Ford stub)
- Records **bus tags** (`hs` / `ms` / `unknown`) so dual-bus work can grow later

## What it does not do (phase 0)

- Module programming or as-built writes
- Full Ford MS-CAN module maps (adapter may report capability; maps come from captures)
- Claim FORScan feature parity

## Install (from source)

```sh
git clone https://github.com/theesfeld/obdtui.git
cd obdtui
cargo build --release -p obd-tui
```

Binary: `target/release/obdtui`

You need `pkg-config` and `libudev` on Linux for serial port access. Add your user to the `dialout` group so the adapter opens without root.

```sh
sudo usermod -aG dialout "$USER"
# log out and log in again
```

## Quick start

List ports (USB serial and `/dev/rfcomm*`):

```sh
obdtui --list-ports
```

### Bluetooth adapter (classic SPP)

Most Bluetooth OBD sticks speak the same ELM protocol as USB. The host sees a serial port after you pair and bind RFCOMM.

1. Unblock Bluetooth if needed: `rfkill unblock bluetooth`
2. Pair and trust the adapter (`bluetoothctl` or your desktop UI)
3. Bind SPP (example; use your adapter MAC):

```sh
sudo rfcomm bind 0 AA:BB:CC:DD:EE:FF
obdtui --port /dev/rfcomm0
```

4. Release when done: `sudo rfcomm release 0`

Typical baud for ELM over RFCOMM is still `38400` (default). If init fails, try `--baud 115200`.

USB serial example:

```sh
obdtui --port /dev/ttyUSB0
```

Replay sample (no truck):

```sh
obdtui --write-sample fixtures/sample-session
obdtui --replay fixtures/sample-session
```

## TUI keys

| Key | Action |
|-----|--------|
| `q` / Esc | Quit |
| `1`–`4` / Tab | Live / DTC / Log / Help |
| `p` | Toggle live poll |
| `r` | Read DTCs |
| `c` | Start or stop capture |
| `b` | Cycle bus tag |
| `x` | Clear DTCs (only with `--allow-writes`) |

## Capture format (for other projects)

A capture directory contains:

| File | Content |
|------|---------|
| `meta.toml` | VIN, adapter, protocol, software version |
| `frames.ndjson` | Timestamped TX/RX with bus tags |
| `signals.csv` | Decoded timeseries |
| `session.json` | Full package (schema in `schemas/`) |
| `vehicle.yaml` | Profile snapshot when present |

Load in Rust with `obd_io::CaptureSession::load(path)`.

## Crates

| Crate | Role |
|-------|------|
| `obd-io` | Transport trait, ELM/STN, J1979, profiles, capture |
| `obd-sim` | Sample fixtures and replay helpers |
| `obd-tui` | `obdtui` binary |

## Safety

- Default mode is read-only.
- Clear DTCs only with `--allow-writes`.
- Do not use this tool to program modules until a later phase with explicit gates.

## License

MIT. See [LICENSE](LICENSE).

## Links

- Site: <https://theesfeld.github.io/obdtui/>
- Issues: <https://github.com/theesfeld/obdtui/issues>
