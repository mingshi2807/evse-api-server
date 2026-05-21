# E2E Test Scenarios — Design, Build & Regeneration Guide

## Overview

The `e2e_scenarios/` directory contains JSON files that drive the EVCC emulator
(`tools/evcc_emulator.py`) through full ISO 15118-20 charging state-machine flows.
Every `"exi_hex"` field contains the actual EXI-encoded bytes extracted from the
libiso15118 test suite — no mock or placeholder data.

Three scenarios cover the three major ISO 15118-20 use cases:

| Scenario | Mode | Payload types | DER controls |
|---|---|---|---|
| `dc_scheduled.json` | DC Scheduled | SAP + Part20Main + Part20DC | None |
| `ac_flow.json` | AC basic | SAP + Part20Main + Part20AC | None |
| `ac_der_iec.json` | AC DER IEC Dynamic | SAP + Part20Main + Part20AC | volt-watt, freq-watt, volt-var, watt-var, watt-cos-phi, FRT, zero-current, DSO Q/cos-phi |

## Where the EXI Payloads Come From

All payloads originate from libiso15118's own EXI encode/decode test suite:

```
libiso15118.git/test/exi/cb/
├── app_hand/app_hand.cpp              # SupportedAppProtocol (SAP)
└── iso20/
    ├── session_setup.cpp              # SessionSetup
    ├── authorization.cpp              # Authorization
    ├── authorization_setup.cpp        # AuthorizationSetup
    ├── service_discovery.cpp          # ServiceDiscovery
    ├── service_selection.cpp          # ServiceSelection
    ├── service_detail.cpp             # ServiceDetail
    ├── schedule_exchange.cpp          # ScheduleExchange
    ├── ac_charge_parameter_discovery.cpp  # AC CPD (basic + DER)
    ├── ac_charge_loop.cpp                 # AC CL  (basic + Scheduled + Dynamic + DER)
    ├── dc_charge_parameter_discovery.cpp  # DC CPD
    ├── dc_cable_check.cpp                 # DC CableCheck
    ├── dc_pre_charge.cpp                  # DC PreCharge
    ├── dc_charge_loop.cpp                 # DC ChargeLoop
    ├── dc_welding_detection.cpp           # DC WeldingDetection
    ├── power_delivery.cpp                 # PowerDelivery
    └── session_stop.cpp                   # SessionStop
```

Each test file contains Catch2 test cases. Some sections use `uint8_t doc_raw[]`
for deserialization tests; others use `std::vector<uint8_t> expected` for
serialization tests; and DER-specific sections use round-trip serialization
(`serialize_helper()`) without a stored expected vector.

## Build Pipeline

### Step 1 — Build libiso15118 with test targets enabled

```bash
cd ~/workspace/libiso15118.git

# Install prerequisites (Ubuntu 24.04)
sudo apt-get install -y cmake ninja-build libssl-dev gcovr clang-15

# Configure with BUILD_TESTING=ON
CC=clang-15 CXX=clang++-15 cmake -S . -B build -G Ninja \
  -DCMAKE_BUILD_TYPE=Release \
  -DBUILD_TESTING=ON \
  -DDISABLE_EDM=ON \
  -DCMAKE_POSITION_INDEPENDENT_CODE=ON \
  -DCMAKE_EXPORT_COMPILE_COMMANDS=ON

# Build all EXI test binaries (17 targets)
cmake --build build \
  --target test_exi_service_discovery test_exi_session_setup \
  --target test_exi_authorization test_exi_authorization_setup \
  --target test_exi_service_selection test_exi_service_detail \
  --target test_exi_schedule_exchange test_exi_power_delivery \
  --target test_exi_session_stop test_app_hand \
  --target test_exi_dc_charge_parameter_discovery test_exi_dc_cable_check \
  --target test_exi_dc_pre_charge test_exi_dc_charge_loop \
  --target test_exi_dc_welding_detection \
  --target test_exi_ac_charge_parameter_discovery \
  --target test_exi_ac_charge_loop \
  -j$(nproc)
```

### Step 2 — Extract EXI payloads

**Method A: Expected vectors (most messages, fast)**

These come from test sections with `std::vector<uint8_t> expected = {...}`.
The Python extraction script parses the test sources and extracts the hex bytes
directly.

Assignment rule: for each source file, the **first** expected vector is the
**Request**, the **second** is the **Response** (serialized SECC-side payload).

**Method B: Round-trip serialization (DER-specific only)**

DER payloads (volt-watt, frequency-watt, volt-var curves, DSO setpoints, etc.)
are tested via round-trip (`serialize_helper` → decode → verify) without
storing an expected vector. To extract these:

1. Temporarily add `fprintf(stderr, "HEX %02x...", ...)` after each `serialize_helper()` call
   in the test sources (ac_charge_loop.cpp, ac_charge_parameter_discovery.cpp).
2. Rebuild only the affected test binaries.
3. Run them and capture stderr via `grep "HEX "`.
4. Restore the original test sources.

This was done once to populate `ac_der_iec.json`; repeat only when the DER
control structures change upstream.

### Step 3 — Generate scenario JSON

The scenario JSON wraps each payload into an EVCC emulator action:

```json
// Request (send)
{"comment": "DER_AC_CPDReq", "action": "send", "payload_type": "0x8004", "exi_hex": "8010041ea6..."}

// Response (expect)
{"comment": "DER_AC_CPDRes", "action": "expect", "payload_type": "0x8004"}

// Wait between exchanges
{"action": "wait", "duration": 0.2}
```

## Scenario JSON Format Reference

```json
[
  {
    "comment": "[0] SupportedAppProtocolReq",
    "action": "send",
    "payload_type": "0x8001",
    "exi_hex": "8000f3ab9371d34b9b79d39ba321..."
  },
  {"action": "wait", "duration": 0.2},
  {
    "comment": "[2] SupportedAppProtocolRes",
    "action": "expect",
    "payload_type": "0x8001"
  },
  {"action": "close", "comment": "flow complete"}
]
```

Available actions:
- `send` — frame EXI hex with V2GTP header, send over TCP
- `expect` — read V2GTP packet, verify payload type matches
- `wait` — sleep for `duration` seconds
- `close` — close TCP connection

## Payload Type Mapping

| Hex | Type | Messages |
|---|---|---|
| `0x8001` | SAP | SupportedAppProtocol |
| `0x8002` | Part20Main | SessionSetup, Auth, ServiceDiscovery/Selection/Detail, ScheduleExchange, PowerDelivery, SessionStop |
| `0x8003` | Part20DC | DC CPD, CableCheck, PreCharge, ChargeLoop, WeldingDetection |
| `0x8004` | Part20AC | AC CPD, AC ChargeLoop (basic + DER) |

## How to Regenerate After libiso15118 Changes

1. Pull latest libiso15118.git changes.
2. Rebuild test targets (Step 1 above).
3. For **most messages** (basic DC/AC): run the Python extraction script via
   `code_execution` in DeepSeek. This re-parses all test sources and regenerates
   `dc_scheduled.json` and `ac_flow.json`.
4. For **DER-specific messages** (`ac_der_iec.json`): if the DER control structures
   changed, repeat Method B (patch test sources, rebuild, grep HEX).
5. Verify each JSON file has valid hex (non-empty, starts with `80` — the EXI
   header byte).
6. Commit the updated scenario files.

## Usage With EVCC Emulator

```bash
# Against a running libiso15118 SECC listener (port 50000)
./tools/evcc_emulator.py \
  --scenario tools/e2e_scenarios/dc_scheduled.json \
  --host 127.0.0.1 --port 50000

# AC DER IEC flow
./tools/evcc_emulator.py \
  --scenario tools/e2e_scenarios/ac_der_iec.json \
  --host 127.0.0.1 --port 50000
```

The emulator frames each `exi_hex` payload with an 8-byte V2GTP header
(protocol version 0x01, inverse 0xFE, payload type, length) and sends it
over TCP. On `expect` steps, it reads the V2GTP header back and verifies
the payload type matches.

Updated (21 / Mai / 2026)
The e2e_scenarios and note.md now cover:
- Source map — which test file provides each message type
- Build pipeline — cmake configuration, 17 EXI test binary targets
- Two extraction methods — expected vectors (fast, most messages) vs. round-trip serialization (DER-specific, requires temporary
  fprintf patching)
- JSON format reference — all 4 action types with examples
- Payload type mapping — SAP / Part20Main / Part20DC / Part20AC
- Regeneration procedure — after upstream libiso15118 changes
- Usage example — evcc_emulator.py invocation for each scenario


## DER Payload Extraction — Concrete HOWTO (Method B)

The `ac_der_iec.json` DER-specific payloads were extracted by temporarily
patching the test sources to print serialized hex bytes. Here's the exact
procedure:

```bash
cd ~/workspace/libiso15118.git

# 1. Backup originals
cp test/exi/cb/iso20/ac_charge_loop.cpp{,.bak}
cp test/exi/cb/iso20/ac_charge_parameter_discovery.cpp{,.bak}

# 2. Patch: add hex dump after each serialize_helper() call
#    These are at 4 locations (2 per file):
#      ac_charge_loop.cpp:353  → DER dynamic AC CL Req
#      ac_charge_loop.cpp:418  → DER dynamic AC CL Res + DSO
#      ac_charge_parameter_discovery.cpp:267  → DER AC CPD Req
#      ac_charge_parameter_discovery.cpp:338  → DER AC CPD Res
#
#    Insert after each "const auto serialized = serialize_helper(req|res);":
#      fprintf(stderr, "HEX ");
#      for (auto b : serialized) fprintf(stderr, "%02x", b);
#      fprintf(stderr, "\n");
#
#    (This was done via code_execution — ask DeepSeek to apply the patch.)

# 3. Rebuild only the two affected test binaries
cmake --build build \
  --target test_exi_ac_charge_loop test_exi_ac_charge_parameter_discovery \
  -j$(nproc)

# 4. Run and capture the hex output
./build/test/exi/cb/iso20/test_exi_ac_charge_loop 2>&1 | grep "^HEX "
./build/test/exi/cb/iso20/test_exi_ac_charge_parameter_discovery 2>&1 | grep "^HEX "

# 5. Restore originals
mv test/exi/cb/iso20/ac_charge_loop.cpp{.bak,}
mv test/exi/cb/iso20/ac_charge_parameter_discovery.cpp{.bak,}
```

Note: a previous attempt at a standalone extractor (`tools/exi_extractor.cpp`)
was removed because it required the private test header `helper.hpp` and could
not be linked from outside the test directory. The test-patching approach above
is the canonical method.


## Dual Build Directories — Production vs Testing

libiso15118 uses **two separate CMake build directories** to avoid coverage
instrumentation leaking into the Rust binary:

```
libiso15118.git/
├── build/          # BUILD_TESTING=ON  (with coverage)
│   ├── src/iso15118/libiso15118.a      ← instrumented, needs libclang_rt.profile
│   └── test/exi/cb/iso20/test_exi_*    ← used for EXI payload extraction
│
└── build_prod/     # BUILD_TESTING=OFF (no coverage)
    ├── src/iso15118/libiso15118.a      ← clean, no extra linker deps
    └── api/c/libiso15118_c.a           ← clean
```

### Why Two Builds?

When `BUILD_TESTING=ON`, the cmake coverage module (`everest-cmake/CodeCoverage.cmake`)
calls `append_coverage_compiler_flags_to_target(iso15118)`, which adds
`-fprofile-arcs -ftest-coverage` (clang) to every object in `libiso15118.a`.
This injects `llvm_gcda_*` symbol references into the archive.

When `cargo run` links the Rust binary against the instrumented `libiso15118.a`,
the linker fails with:

```
undefined symbol: llvm_gcda_start_file
undefined symbol: llvm_gcda_emit_function
undefined symbol: llvm_gcda_emit_arcs
undefined symbol: llvm_gcda_summary_info
undefined symbol: llvm_gcda_end_file
undefined symbol: llvm_gcov_init
```

These symbols live in `libclang_rt.profile-x86_64.a` (compiler-rt), which is
not on the Rust link line. The fix: point the Rust `build.rs` at `build_prod/`.

### Setup Commands

```bash
cd ~/workspace/libiso15118.git

# Production build (for Rust FFI)
CC=clang-15 CXX=clang++-15 cmake -S . -B build_prod -G Ninja \
  -DCMAKE_BUILD_TYPE=Release \
  -DBUILD_TESTING=OFF \
  -DDISABLE_EDM=ON \
  -DCMAKE_POSITION_INDEPENDENT_CODE=ON
cmake --build build_prod --target iso15118 iso15118_c -j$(nproc)

# Testing build (for EXI payload extraction)
CC=clang-15 CXX=clang++-15 cmake -S . -B build -G Ninja \
  -DCMAKE_BUILD_TYPE=Release \
  -DBUILD_TESTING=ON \
  -DDISABLE_EDM=ON \
  -DCMAKE_POSITION_INDEPENDENT_CODE=ON \
  -DCMAKE_EXPORT_COMPILE_COMMANDS=ON
cmake --build build --target ...test_exi_*... -j$(nproc)
```

The Rust `build.rs` then links against `build_prod/`:

```rust
let build_dir = lib_dir.join("build_prod");  // NOT "build"
```

### Quick Fix (if you accidentally built only `build/` with testing)

If the `build_prod/` directory doesn't exist and `build/` has coverage, you can
temporarily link the clang runtime profile library by adding to `build.rs`:

```rust
println!("cargo:rustc-link-search=native=/usr/lib/llvm-15/lib/clang/15.0.7/lib/linux");
println!("cargo:rustc-link-lib=static=clang_rt.profile-x86_64");
```

But the dual-directory approach is preferred — it keeps coverage out of the
Rust link line and avoids hardcoding clang version paths.
