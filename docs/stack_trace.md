# evse-api-server — Full Stack Architecture

## Compile-time Pipeline

```
cargo build
├── Step 1: build.rs executes
│   ├── cc::Build compiles api/c/iso15118_c.cpp → libiso15118_c.a
│   └── emits cargo:rustc-link-lib for iso15118 + 3× cbv2g archives
├── Step 2: rustc compiles evse-api-core (rlib)
│   ├── ffi.rs    → extern "C" declarations
│   ├── session.rs→ Safe wrapper around C handle
│   ├── manager.rs→ tokio poll loop (50ms)
│   └── protocol.rs→ serde JSON types
├── Step 3: rustc compiles evse-api-server (binary)
│   ├── main.rs   → tokio::main, TcpListener, axum
│   └── server.rs → WebSocket upgrade handler
└── Step 4: rust-lld linking
    Inputs:
      libevse_api_server-*.rlib   (Rust code)
      libevse_api_core-*.rlib     (Rust code)
      libiso15118_c.a             (C wrapper, compiled by cc-rs)
      libiso15118.a               (C++ library, built by cmake)
      libcbv2g_iso20.a            (EXI message codecs)
      libcbv2g_exi_codec.a        (EXI bitstream primitives)
      libcbv2g_tp.a               (V2GTP header read/write)
      libssl.a + libcrypto.a      (system)
      libstdc++.so                (system)
    → evse-api-server (ELF binary)
```

## Layer Map

```
Layer 4: WebSocket Server (axum)
  evse-api-server/src/{main.rs, server.rs}
  - Tokio async runtime, ws://0.0.0.0:8080/ws
  - JSON command dispatch → SessionManager

Layer 3: Session Manager (evse-api-core)
  manager.rs: tokio::spawn poll loop, 50ms tick
  session.rs: Safe Rust wrapper around C handle
  protocol.rs: serde JSON types ↔ ControlEvent enum

Layer 2: C FFI Bridge
  ffi.rs: unsafe extern "C" declarations
  ↔ iso15118_c.h / iso15118_c.cpp
  - Opaque handle: iso15118_session_t
  - 7 C functions with C linkage
  - Callback: C → Rust via tokio mpsc channel

Layer 1: libiso15118 (C++)
  session/iso.cpp: Session::poll(), FSM, V2GTP reads/writes
  message/*.cpp: EXI serialize/deserialize via libcbv2g
  io/connection_plain.cpp: TCP sockets + PollManager

Layer 0: libcbv2g (C)
  exi_bitstream.c: EXI bit-level encode/decode
  exi_v2gtp.c: V2GTP header Read/Write
  iso20_AC/DC/CommonMessages_*: ISO 15118-20 message codecs
```

## Why Three libcbv2g Archives Are Needed

CMake static libraries don't merge their dependencies. When CMakeLists.txt has:

```cmake
target_link_libraries(iso15118 PUBLIC cbv2g::cbv2g_iso20 cbv2g::cbv2g_exi_codec cbv2g::cbv2g_tp)
```

This records a dependency but does NOT inline libcbv2g objects into libiso15118.a. The build produces three separate archives. The Rust linker needs all of them because:

- `iso15118_c.cpp` includes headers that transitively pull in `<cbv2g/common/exi_bitstream.h>`
- Template functions like `serialize_helper<AC_ChargeLoopResponse>()` call `exi_bitstream_get_length()` (libcbv2g_exi_codec.a)
- `sdp_server.cpp` calls `V2GTP20_ReadHeader()` / `V2GTP20_WriteHeader()` (libcbv2g_tp.a)
- Every message type has `init_iso20_*` and `encode_iso20_*` functions (libcbv2g_iso20.a)

Without the explicit `cbv2g_*` lines in `build.rs`, the linker sees undefined symbols and fails.

## Runtime Data Flow

```
                    EV (Electric Vehicle)
                         │ TCP port 50000 (V2GTP + EXI)
                         ▼
┌──────────────────────────────────────────────────────────┐
│  libiso15118 (C++)                                       │
│  ConnectionPlain::handle_connect() → accept TCP          │
│  Session::poll() [called by C wrapper]                   │
│    → read_single_sdp_packet() → decode EXI → Variant    │
│    → V2GTP_MESSAGE → FSM::feed() → state handles req    │
│    → ctx.respond() → encode EXI → write TCP             │
│  Feedback callbacks fire on poll thread:                 │
│    signal(), v2g_message(), selected_service_params()... │
└──────────────────────┬───────────────────────────────────┘
                       │ C function pointer callback
                       ▼
┌──────────────────────────────────────────────────────────┐
│  libiso15118_c.a (C wrapper)                             │
│  make_callbacks() → feedback::Callbacks                  │
│    serialize each event to JSON                          │
│    event_fn(userdata, json_string)  // → Rust!           │
│  iso15118_session_create(config_json) → opaque handle    │
│  iso15118_session_poll(handle) → wakeup delay or -1      │
│  iso15118_session_push_event(handle, json) → ControlEvent│
└──────────────────────┬───────────────────────────────────┘
                       │ C → Rust via mpsc channel
                       ▼
┌──────────────────────────────────────────────────────────┐
│  evse-api-core (Rust)                                    │
│  Session::new(config_json)                               │
│    → unsafe { iso15118_session_create(c_json) }          │
│    → create tokio mpsc channel                           │
│    → register callback: unsafe extern "C" fn → tx.send() │
│  Session::poll() → unsafe { iso15118_session_poll() }    │
│  SessionManager: tokio::spawn poll loop (50ms tick)      │
└──────────────────────┬───────────────────────────────────┘
                       │ tokio mpsc channels (JSON Strings)
                       ▼
┌──────────────────────────────────────────────────────────┐
│  evse-api-server (Rust binary)                           │
│  UPGRADE /ws → WebSocket                                 │
│  tokio::select! {                                        │
│    event_rx.recv() → socket.send(event)                  │
│    socket.recv() → Command::ControlEvent                 │
│      → serde_json::to_string(&event)                     │
│      → manager.push_event(id, json)                      │
│      → iso15118_session_push_event(...)                  │
│  }                                                       │
└──────────────────────┬───────────────────────────────────┘
                       │ WebSocket
                       ▼
              EVSE Application (remote)
```

## Memory Ownership Across FFI

```
Rust                          C++                        libcbv2g
─────                          ───                        ────────
Session                        iso15118_session_t
  ptr ───────────────────────→ poll_manager (owned)
  _event_tx                     sdp (owned)
                                session: unique_ptr<Session>
                                  connection, fsm, ctx
                                    feedback::Callbacks
                                      → lambda → Rust fn ptr

Drop impl                      iso15118_session_destroy
  destroy(ptr) ──────────────→ delete session_t
                                all unique_ptrs cascade free

Callback (C → Rust)            libcbv2g objects:
  tx.send(json) ←──────────── event_fn(userdata, json)     exi_bitstream_t
  Box::leaked                   lambdas in Callbacks       (stack alloc, temp)
  → channel → WebSocket
```

## Thread Model

```
tokio runtime (multi-threaded)
│
├── Worker #1: axum accept loop + WebSocket handlers
├── Worker #2: SessionManager poll task (50ms tick)
│   ├── Session::poll() → C++ Session::poll()
│   ├── C++ PollManager::poll(50) blocks up to 50ms
│   └── C callbacks fire ON THIS THREAD → mpsc channel
└── Worker #3..N: other async tasks
```

All C++ code runs on a single tokio worker via the SessionManager task — matching libiso15118's single-threaded design. No locks needed for session state. Only the `mpsc::unbounded_channel` bridges tasks (lock-free).
