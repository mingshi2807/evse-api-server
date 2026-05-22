# API E2E Codex Review

Date: 2026-05-22

## Scope

This report documents the first Codex audit of the temporary AC_DER_IEC E2E
test setup across:

- `../evse-api-server.git`: Rust WebSocket API server, Rust FFI wrapper layer,
  test tooling, and scenario documentation.
- `../libiso15118.git`: temporary `e2e` branch used by the Rust server through
  the C FFI wrapper and static libraries.

The objective is to understand the current integration test surface before the
next implementation step, not to approve production readiness.

## Current Repository State

### evse-api-server.git

- Branch: `main`
- Recent work: Rust FFI server, WebSocket session activation, EXI scenario
  tooling, and AC_DER_IEC scenario iteration.
- New local file created during this audit: `AGENTS.md`.
- Working tree before this report: only `AGENTS.md` was untracked.

### libiso15118.git

- Branch: `e2e`
- Recent work: 19 commits after the `api` baseline.
- The branch contains temporary hardcoded behavior to prove E2E flow progress.
- Untracked build folders exist locally: `build-local-der/`, `build-pin-der/`,
  and `note_tmp.txt`.

## evse-api-server Architecture Summary

The Rust server is a thin application layer around libiso15118:

1. `crates/evse-api-server/src/main.rs`
   - Starts Tokio runtime.
   - Starts Axum server.
   - Exposes WebSocket endpoint `/ws`.
   - Exposes HTTP health/status endpoints.

2. `crates/evse-api-server/src/server.rs`
   - Creates one libiso15118 session when a WebSocket client connects.
   - Sends initial `status/connected` JSON event.
   - Forwards C++ callback JSON events to the WebSocket client.
   - Forwards incoming WebSocket `control_event` messages to the session manager.

3. `crates/evse-api-core/src/manager.rs`
   - Owns active sessions.
   - Runs a Tokio poll loop every 50 ms.
   - Calls the C FFI session poll function.
   - Removes finished sessions and emits `session_closed`.

4. `crates/evse-api-core/src/session.rs`
   - Wraps the opaque C session pointer.
   - Registers the C callback.
   - Converts C callback strings into Tokio channel events.

5. `crates/evse-api-core/build.rs`
   - Compiles `../libiso15118.git/api/c/iso15118_c.cpp`.
   - Links `libiso15118.a`, `libiso15118_c.a`, and `libcbv2g_*` archives.

6. `tools/evcc_emulator.py`
   - Opens a TCP connection to the libiso15118 SECC listener.
   - Sends V2GTP-framed EXI payloads from JSON scenarios.
   - Checks response payload type only.

## Test Tooling And Scenario Surface

Scenario files are under `tools/e2e_scenarios/`:

| Scenario | Steps | Send | Expect | Purpose |
|---|---:|---:|---:|---|
| `dc_scheduled.json` | 57 | 14 | 14 | DC scheduled baseline flow |
| `ac_flow.json` | 45 | 11 | 11 | Basic AC baseline flow |
| `ac_der_iec.json` | 47 | 12 | 12 | AC_DER_IEC dynamic EIM flow |

`ac_der_iec.json` currently covers:

- SupportedAppProtocol
- SessionSetup
- Authorization
- ServiceDiscovery
- ServiceDetail
- ServiceSelection
- DER_AC_CPD
- ScheduleExchange
- PowerDelivery
- DER_AC_CL dynamic
- second PowerDelivery
- SessionStop

Important limitation: the emulator only validates the returned V2GTP payload
type. It does not decode EXI responses or validate semantic fields such as
`responseCode`, selected service, selected control functions, DER control
content, or session id.

## Build And Verification Evidence

Commands executed in `../evse-api-server.git`:

```bash
cargo fmt --all --check
cargo check --workspace
cargo clippy --all-targets --all-features -- -D warnings
cargo test --workspace
```

Results:

- `cargo fmt --all --check`: passed.
- `cargo check --workspace`: passed.
- `cargo clippy --all-targets --all-features -- -D warnings`: passed.
- `cargo test --workspace`: failed at native link stage.

Failure reason:

```text
could not find native static library `iso15118`
rust-lld: error: unable to find library -liso15118
rust-lld: error: unable to find library -lcbv2g_iso20
rust-lld: error: unable to find library -lcbv2g_exi_codec
rust-lld: error: unable to find library -lcbv2g_tp
```

Root cause:

- `crates/evse-api-core/build.rs` expects static libraries in
  `../libiso15118.git/build_prod`.
- Local `../libiso15118.git/build_prod` does not exist.
- Available local archive directories are `build-local-der/` and `build-pin-der/`.

## libiso15118 E2E Branch Audit

The `e2e` branch contains useful C FFI/applicative glue, but also many temporary
hardcoded changes. These must be separated before production integration.

### Production-Relevant Glue

Potentially reusable:

- `api/c/iso15118_c.h`
  - Defines opaque `iso15118_session_t`.
  - Exposes create/destroy/poll/push-event/set-callback/close/error functions.

- `api/c/iso15118_c.cpp`
  - Parses minimal JSON config.
  - Creates `ConnectionPlain`.
  - Builds feedback callbacks.
  - Bridges libiso15118 callbacks to JSON events.
  - Accepts control events from Rust.

- `api/c/CMakeLists.txt`
  - Builds `iso15118_c` as a static library.
  - Links it to `iso15118` and OpenSSL.

This FFI layer is the right integration direction, but it needs hardening before
production use.

### Temporary E2E Bypasses

The following changes are explicit test hacks and must not be treated as
production behavior.

1. Session id validation is bypassed.

File:

```text
../libiso15118.git/src/iso15118/d20/context_helper.cpp
```

Behavior:

- `validate_and_setup_header()` accepts any request session id.
- This allows replaying hardcoded test payloads whose session id does not match
  the SECC-generated session id.

Risk:

- Breaks protocol session integrity.
- Would hide real session correlation bugs.

2. EIM authorization is auto-accepted.

Files:

```text
../libiso15118.git/src/iso15118/d20/state/authorization.cpp
../libiso15118.git/api/c/iso15118_c.cpp
```

Behavior:

- `Authorization::enter()` sets authorization status to `Accepted`.
- The C wrapper pre-pushes an authorization event after session creation.
- Pending EIM processing is converted to finished/OK.

Risk:

- Bypasses real EVSE authorization application logic.
- Prevents testing negative authorization paths.

3. ServiceDetail accepts unknown service ids.

File:

```text
../libiso15118.git/src/iso15118/d20/state/service_detail.cpp
```

Behavior:

- Invalid service ids are forced to be considered found.

Risk:

- Hides service advertisement/selection consistency failures.

4. ServiceSelection parameter-set validation is bypassed.

File:

```text
../libiso15118.git/src/iso15118/d20/state/service_selection.cpp
```

Behavior:

- `find_energy_parameter_set_id()` is disabled through `if (false && ...)`.

Risk:

- Hides mismatch between ServiceDetail response parameter sets and
  ServiceSelection request.

5. AC CPD accepts AC_BPT as DER service and forces OK.

File:

```text
../libiso15118.git/src/iso15118/d20/state/ac_charge_parameter_discovery.cpp
```

Behavior:

- DER AC request is accepted when selected service is `AC_DER` or `AC_BPT`.
- DER provider diagnostic failures are ignored.
- Response code is forced to `OK`.

Risk:

- Hides the exact client feedback issue previously raised: DER and BPT data
  modeling can be confused.
- Prevents validation of mandatory AC_DER_IEC control-function consistency.

6. AC contactor close is auto-forced.

File:

```text
../libiso15118.git/src/iso15118/d20/state/power_delivery.cpp
```

Behavior:

- AC connector is marked closed without waiting for the application control
  event.
- Second PowerDelivery request is treated as stop/session transition.

Risk:

- Bypasses EVSE power electronics application contract.
- Prevents realistic timing/state validation.

7. Debug `fprintf(stderr, ...)` traces are embedded in protocol states.

Files include:

```text
authorization.cpp
service_discovery.cpp
schedule_exchange.cpp
ac_charge_parameter_discovery.cpp
iso15118_c.cpp
```

Risk:

- Useful during current E2E debugging.
- Should be replaced by structured logging or removed before merge.

## evse-api-server Risks

### Hardcoded Session Configuration

File:

```text
crates/evse-api-server/src/server.rs
```

Current config is hardcoded in `handle_ws()`:

```json
{
  "evse_id": "default",
  "interface": "lo",
  "energy_services": ["AC_DER"],
  "auth_services": ["EIM"],
  "control_mode": "Dynamic",
  "mobility_mode": "ProvidedByEvcc",
  "ac_limits": {
    "max_charge_power": 22080,
    "min_charge_power": 20000
  }
}
```

Risk:

- Good for current AC_DER_IEC E2E.
- Not yet a real application API contract.

### WebSocket Command Model Is Partial

`protocol.rs` defines `configure`, `start`, `stop`, and `subscribe`, but
`server.rs` currently only handles `control_event`.

Risk:

- The API shape suggests runtime configurability that does not yet exist.

### Callback Userdata Leak

File:

```text
crates/evse-api-core/src/session.rs
```

Behavior:

- `Box::into_raw()` is used for callback userdata.
- The raw box is not reclaimed in `Drop`.

Risk:

- Acceptable for short-lived E2E experiments.
- Must be fixed before long-running production server use.

### Session Status Is Placeholder

File:

```text
crates/evse-api-server/src/server.rs
```

Behavior:

- `/api/v1/status` always returns `"sessions": 0`.

Risk:

- Not reliable for operational monitoring.

### E2E Assertions Are Too Shallow

File:

```text
tools/evcc_emulator.py
```

Behavior:

- Only validates V2GTP payload type.

Risk:

- A protocol response with wrong semantic content can still pass the scenario.

## Readiness Position

Current maturity level:

- Good for proving that Rust WebSocket -> C FFI -> libiso15118 -> TCP EVCC
  transport can be exercised.
- Not yet sufficient for production SECC application integration.
- Not yet sufficient for AC_DER_IEC conformance-style validation.

The current E2E branch validates integration plumbing more than normative
protocol correctness.

## Recommended Next Steps

### Step 1: Make Build Directory Selection Explicit

Problem:

- Rust build expects `../libiso15118.git/build_prod`.
- Local usable build directories are `build-local-der/` and `build-pin-der/`.

Recommendation:

- Add an environment override, for example:

```bash
LIBISO15118_BUILD_DIR=../libiso15118.git/build-pin-der cargo test --workspace
```

Implementation target:

- `crates/evse-api-core/build.rs`

Expected benefit:

- Reproducible local and CI builds.
- No hard dependency on a single build directory name.

### Step 2: Build Or Rebuild Non-Coverage Static Archives

Ensure the selected libiso15118 build directory contains:

```text
src/iso15118/libiso15118.a
api/c/libiso15118_c.a
_deps/libcbv2g-build/lib/cbv2g/libcbv2g_iso20.a
_deps/libcbv2g-build/lib/cbv2g/libcbv2g_exi_codec.a
_deps/libcbv2g-build/lib/cbv2g/libcbv2g_tp.a
```

### Step 3: Run Manual Three-Terminal AC_DER_IEC E2E

Terminal 1:

```bash
RUST_LOG=info cargo run -p evse-api-server
```

Terminal 2:

```bash
websocat ws://localhost:8080/ws
```

Terminal 3:

```bash
./tools/evcc_emulator.py \
  --scenario tools/e2e_scenarios/ac_der_iec.json \
  --host ::1 \
  --port 50000
```

Expected evidence to capture:

- WebSocket `connected` event.
- libiso15118 TCP listener active on port 50000.
- EVCC emulator reaches `SessionStopRes`.
- Rust server forwards relevant libiso15118 events.
- C++ logs show intended state sequence.

### Step 4: Strengthen E2E Assertions

Upgrade the emulator or add a decoder step so tests can validate:

- response code is `OK` where expected
- selected service is AC_DER, not AC_BPT
- selected control mode is Dynamic
- CPD response includes DER AC transfer mode
- ChargeLoop response includes expected DER control fields
- session id behavior is understood and eventually not bypassed

### Step 5: Separate E2E Hacks From Production Backlog

Keep the temporary branch for proving flow, but create a production backlog to
replace each bypass with a real API/application control contract:

- authorization provider contract
- contactor control contract
- AC DER provider/config injection contract
- service selection and parameter-set correctness
- response semantic validation
- callback coverage and structured diagnostics

## Stop Condition For This Audit

This audit is complete when:

- The architecture and current branch state are documented.
- Temporary test hacks are separated from production-ready glue.
- The immediate build blocker is identified.
- The next reproducible validation step is clear.

This report satisfies that stop condition.
