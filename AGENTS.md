# Repository Guidelines

## Purpose

This repository is a Rust WebSocket application layer used to drive and test
`../libiso15118.git` through its C FFI wrapper. The immediate focus is AC_DER_IEC
end-to-end validation with real ISO 15118-20 EXI payloads from libiso15118 tests.

## Project Structure

- `crates/evse-api-core/`: FFI bindings, session wrapper, protocol DTOs, and `SessionManager`.
- `crates/evse-api-server/`: Axum HTTP/WebSocket server and session activation.
- `tools/`: smoke/e2e shell scripts, EVCC TCP emulator, and scenario JSON files.
- `tools/e2e_scenarios/`: real EXI payload flows for DC scheduled, AC, and AC_DER_IEC.
- `docs/`: architecture and operational notes for build/link/runtime flows.

## Build And Test

- Format: `cargo fmt --all --check`
- Lint: `cargo clippy --all-targets --all-features -- -D warnings`
- Build: `cargo build --release`
- Smoke: `./tools/smoke_test.sh`
- API e2e smoke: `./tools/e2e_test.sh`
- Manual AC_DER_IEC scenario:
  1. Start server: `RUST_LOG=info cargo run -p evse-api-server`
  2. Keep a WebSocket client open: `websocat ws://localhost:8080/ws`
  3. Run EVCC emulator: `./tools/evcc_emulator.py --scenario tools/e2e_scenarios/ac_der_iec.json --host ::1 --port 50000`

## Integration Constraints

- `crates/evse-api-core/build.rs` expects `../libiso15118.git/build_prod` and links
  `libiso15118.a`, `libiso15118_c.a`, and the three `libcbv2g` static archives.
- Use `build_prod` for Rust linking to avoid coverage/GCDA instrumentation issues.
- The WebSocket connection is the session activator; libiso15118 TCP port 50000 is
  opened only after a WebSocket client connects.
- Keep `websocat` or another WS client open while running `tools/evcc_emulator.py`.
- AC_DER_IEC test work may depend on temporary hardcoded behavior in libiso15118
  branch `e2e`; do not treat those hacks as production behavior.

## Coding Guidance

- Follow existing Rust 2024 workspace patterns and keep unsafe FFI isolated in
  `evse-api-core`.
- Prefer typed protocol structs in `protocol.rs` over ad hoc JSON construction.
- Keep blocking C++ polling isolated in `SessionManager`; do not call FFI directly
  from WebSocket handlers except through `Session`/`SessionManager`.
- Do not regenerate scenario payloads casually; `tools/e2e_scenarios/note.md`
  documents the payload source and regeneration process.

## Review Focus

- Verify WebSocket session lifecycle, FFI ownership, C callback safety, and whether
  scenario ordering matches the libiso15118 FSM.
- Separate production-capable API glue from temporary e2e bypasses in
  `../libiso15118.git`.
- Report exact commands and outcomes when claiming readiness.
