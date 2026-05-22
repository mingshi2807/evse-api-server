# EVSE API Server

Rust WebSocket application layer for exercising `../libiso15118.git` through a
C FFI wrapper. The current priority is to test the new AC_DER_IEC implementation
in `libiso15118.git` with reproducible end-to-end scenarios.

This repository is not yet a production EVSE API. It is currently an integration
and validation harness that connects:

```text
WebSocket client
  -> evse-api-server Rust API
  -> evse-api-core Rust session manager
  -> libiso15118 C wrapper
  -> libiso15118 C++ ISO 15118-20 state machine
  -> TCP/V2GTP/EXI EVCC emulator
```

## Main Purpose

The goal is to validate that an external SECC application can control and
observe `libiso15118.git` through a stable API boundary while running real
ISO 15118-20 EXI payloads.

The first focused use case is:

```text
ISO 15118-20 AMD1 AC_DER_IEC
Authorization: EIM
Control mode: Dynamic
Mobility mode: ProvidedByEvcc
Transport: WebSocket API + TCP V2GTP EVCC emulator
```

## Repository Layout

```text
crates/
  evse-api-core/
    build.rs        # links against ../libiso15118.git static archives
    src/ffi.rs      # unsafe C FFI declarations
    src/session.rs  # safe-ish Rust wrapper around opaque C session
    src/manager.rs  # Tokio session poll loop
    src/protocol.rs # JSON command/event DTOs

  evse-api-server/
    src/main.rs     # Axum/Tokio server startup
    src/server.rs   # HTTP + WebSocket routes

tools/
  evcc_emulator.py              # TCP EVCC emulator, sends V2GTP-framed EXI
  smoke_test.sh                 # HTTP/WebSocket smoke test
  e2e_test.sh                   # API-level integration smoke test
  e2e_scenarios/
    dc_scheduled.json
    ac_flow.json
    ac_der_iec.json
    note.md                     # scenario generation and usage notes

docs/
  stack_trace.md                # architecture, linking, and runtime flow notes

api_e2e_codexReview.md          # current audit report
AGENTS.md                       # Codex/OMX operating guide for this repo
```

## External Dependency

This repository expects a sibling checkout:

```text
../libiso15118.git
```

The Rust build links against static libraries built from that repository:

```text
libiso15118.a
libiso15118_c.a
libcbv2g_iso20.a
libcbv2g_exi_codec.a
libcbv2g_tp.a
```

Current `build.rs` expects them under:

```text
../libiso15118.git/build_prod
```

If that directory is missing, link-stage commands such as `cargo test` will fail
even when Rust type-checking succeeds.

## Prerequisites

Install typical local tools:

```bash
sudo apt-get install -y cmake ninja-build libssl-dev clang websocat curl
```

Rust toolchain:

```bash
rustup default stable
rustup component add rustfmt clippy
```

Python is only needed for the current EVCC emulator:

```bash
python3 --version
```

## Prepare libiso15118 Static Libraries

From `../libiso15118.git`, prepare a non-coverage build for Rust linking:

```bash
cd ../libiso15118.git

cmake -S . -B build_prod -G Ninja \
  -DCMAKE_BUILD_TYPE=Release \
  -DBUILD_TESTING=OFF \
  -DDISABLE_EDM=ON \
  -DCMAKE_POSITION_INDEPENDENT_CODE=ON \
  -DCMAKE_EXPORT_COMPILE_COMMANDS=ON

cmake --build build_prod -j"$(nproc)"
```

Confirm these files exist:

```bash
find build_prod -type f \( \
  -name 'libiso15118*.a' -o \
  -name 'libcbv2g*.a' \
\) | sort
```

Expected important outputs:

```text
build_prod/src/iso15118/libiso15118.a
build_prod/api/c/libiso15118_c.a
build_prod/_deps/libcbv2g-build/lib/cbv2g/libcbv2g_iso20.a
build_prod/_deps/libcbv2g-build/lib/cbv2g/libcbv2g_exi_codec.a
build_prod/_deps/libcbv2g-build/lib/cbv2g/libcbv2g_tp.a
```

## Build And Static Checks

From this repository:

```bash
cargo fmt --all --check
cargo check --workspace
cargo clippy --all-targets --all-features -- -D warnings
cargo build --release
```

Known current audit result:

- `cargo fmt --all --check`: passed
- `cargo check --workspace`: passed
- `cargo clippy --all-targets --all-features -- -D warnings`: passed
- `cargo test --workspace`: blocked if `../libiso15118.git/build_prod` does not exist

## Run API Smoke Tests

```bash
./tools/smoke_test.sh
```

This validates:

- server startup
- `/api/v1/health`
- `/api/v1/status`
- WebSocket connection
- initial session status event

## Run Manual AC_DER_IEC E2E Test

The current manual test uses three terminals.

### Terminal 1: Start API Server

```bash
RUST_LOG=info cargo run -p evse-api-server
```

The Rust server listens on:

```text
http://localhost:8080
ws://localhost:8080/ws
```

### Terminal 2: Keep WebSocket Session Open

```bash
websocat ws://localhost:8080/ws
```

This is required because the WebSocket connection creates the libiso15118
session. The ISO 15118 TCP listener on port `50000` is opened only after session
creation.

Expected first event:

```json
{"type":"status","message":"connected","session_id":"..."}
```

### Terminal 3: Run EVCC Emulator

```bash
./tools/evcc_emulator.py \
  --scenario tools/e2e_scenarios/ac_der_iec.json \
  --host ::1 \
  --port 50000
```

Expected high-level result:

```text
[EVCC] Done: <n> passed, 0 failed
```

Important: this currently proves transport/session flow only. The emulator
checks V2GTP payload type, but it does not yet decode EXI response fields.

## Current AC_DER_IEC Scenario

`tools/e2e_scenarios/ac_der_iec.json` contains a dynamic EIM AC_DER_IEC flow:

```text
SupportedAppProtocol
SessionSetup
Authorization
ServiceDiscovery
ServiceDetail
ServiceSelection
DER_AC_CPD
ScheduleExchange
PowerDelivery
DER_AC_CL Dynamic
PowerDelivery Stop
SessionStop
```

The scenario uses real EXI bytes extracted from libiso15118 test vectors and
documented in `tools/e2e_scenarios/note.md`.

## Current Limitations

The current test setup is useful but not yet production-grade.

Known limitations:

- `crates/evse-api-server/src/server.rs` hardcodes AC_DER_IEC session config.
- WebSocket commands such as `configure`, `start`, `stop`, and `subscribe` are
  defined in DTOs but not fully implemented in the WebSocket handler.
- `/api/v1/status` currently returns a placeholder session count.
- `tools/evcc_emulator.py` checks only V2GTP payload type, not decoded EXI
  semantic content.
- `crates/evse-api-core/src/session.rs` leaks callback userdata allocated with
  `Box::into_raw`; acceptable for short E2E runs, not for production.
- The current `../libiso15118.git/e2e` branch includes deliberate temporary
  bypasses for session id, EIM authorization, service selection, AC contactor
  closure, and AC CPD response handling.

## Audit Report

See:

```text
api_e2e_codexReview.md
```

That file documents the current Codex audit in more detail, including positive
points, negative points, vulnerabilities, and the immediate reproducibility
blocker.

## Status

### Positive

- Rust workspace structure is clear and small.
- FFI boundary is isolated in `evse-api-core`.
- WebSocket session activation works as a practical test-control surface.
- Scenario tooling exists for DC scheduled, AC, and AC_DER_IEC flows.
- `cargo fmt`, `cargo check`, and `cargo clippy -D warnings` passed during the
  first audit.

### Negative

- Full test execution is not reproducible until the expected libiso15118 static
  archive directory is available.
- AC_DER_IEC E2E currently depends on temporary hardcoded behavior in
  `../libiso15118.git/e2e`.
- The EVCC emulator does not yet verify decoded protocol semantics.
- The server API contract is incomplete and currently test-harness oriented.

### Vulnerabilities And Risks

- Session id validation is bypassed in the temporary libiso15118 e2e branch.
- Authorization and contactor control can be auto-accepted/auto-closed in the
  current e2e branch.
- DER provider failures can be hidden by forced `OK` AC CPD responses.
- Callback userdata lifetime is not production-safe.
- Silent WebSocket command parsing failures can hide malformed client commands.

## Roadmap TODO

Priority focus: properly E2E-test `../libiso15118.git` with the new
AC_DER_IEC implementation.

- [ ] Make libiso15118 build directory configurable from Rust, for example with
      `LIBISO15118_BUILD_DIR`, instead of hardcoding `build_prod`.
- [ ] Rebuild or select a non-coverage libiso15118 build containing all required
      static archives.
- [ ] Run `cargo test --workspace` successfully with the selected libiso15118
      build.
- [ ] Run the manual three-terminal AC_DER_IEC scenario and capture logs from
      Rust, WebSocket, EVCC emulator, and libiso15118.
- [ ] Add an automated AC_DER_IEC E2E script that starts server, keeps WebSocket
      open, runs the EVCC emulator, and performs cleanup.
- [ ] Upgrade the EVCC emulator to decode or verify EXI semantic fields, not only
      V2GTP payload type.
- [ ] Assert AC_DER_IEC-specific facts: selected service is AC_DER, selected mode
      is Dynamic, DER CPD response uses DER AC transfer mode, and ChargeLoop
      response contains expected DER control content.
- [ ] Replace temporary libiso15118 e2e bypasses with real application-layer
      control events and explicit API contracts.
- [ ] Implement runtime configuration through WebSocket or HTTP instead of
      hardcoded `cfg_json`.
- [ ] Fix callback userdata ownership and session lifecycle cleanup.
- [ ] Add negative scenarios: wrong service, wrong parameter set, missing
      mandatory DER control functions, authorization rejected, contactor timeout.
- [ ] Convert audit evidence into CI-friendly checks once the local E2E flow is
      stable.
