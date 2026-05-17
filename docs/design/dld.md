# otelc — Detailed Design

This document describes the implementation. It tracks the code in this
repository; file paths are relative to the workspace root.

## 1. Tech stack and crate selection

| Crate | Version | Why |
|-------|---------|-----|
| `ratatui` | 0.30 | Terminal UI widgets, layout, rendering. |
| `crossterm` | 0.29 | Cross-platform terminal backend (re-exported via `ratatui`). |
| `tokio` | 1 | Async runtime for the server, receiver and event loop. |
| `tokio-tungstenite` | 0.29 | OpAMP WebSocket transport (server and test client). |
| `prost` / `prost-build` | 0.14 | Generate OpAMP types from `opamp.proto`. |
| `tonic` | 0.14 | gRPC server hosting the embedded OTLP receiver. |
| `opentelemetry-proto` | 0.32 | OTLP message types + tonic service stubs (`gen-tonic`). |
| `serde` / `serde_yaml_ng` | 1 / 0.10 | Parse the effective-config YAML for the pipeline graph. |
| `uuid` | 1 | UUIDv7 instance UIDs. |
| `clap` | 4 | Command-line parsing. |
| `tracing` (+ `-subscriber`, `-appender`) | 0.1 | Diagnostics to a log file. |
| `anyhow` / `thiserror` | 1 / 2 | Application and typed errors. |

No mature Rust OpAMP *server* crate exists, so the OpAMP wire types are
generated from a vendored `opamp.proto` and the server is implemented directly.
OTLP protos are **not** vendored — `opentelemetry-proto` ships both the message
types and the tonic gRPC service stubs.

## 2. Process and async model

`otelc` is a single binary on one multi-threaded `tokio` runtime. Long-lived
async tasks: the OpAMP server accept loop, one task per agent connection, the
OTLP gRPC server, the OpAMP→`ControlEvent` translator, and the telemetry poll.

The UI runs on the main task. Terminal input is read on a dedicated OS thread
(`crossterm` `read()` is blocking) and forwarded over a channel. The event loop
`select!`s over three sources: terminal input, `ControlEvent`s, and a 250 ms
render tick. **All UI state mutation is single-threaded** inside that loop;
backends communicate only through channels, so the UI never blocks on I/O and
needs no locks on its own state.

## 3. Workspace and module map

```
otelc/
├─ Cargo.toml                       workspace + [workspace.dependencies]
├─ rust-toolchain.toml               pinned stable toolchain
├─ proto/opamp.proto, anyvalue.proto vendored OpAMP schema (+ provenance README)
├─ otelc.example.yaml                example config file
├─ crates/
│  ├─ otelc-opamp/                   OpAMP wire types, framing, embedded server
│  │  ├─ build.rs                    prost-build codegen for opamp.proto
│  │  └─ src/
│  │     ├─ lib.rs                   crate root; `pb` = generated types
│  │     ├─ framing.rs               WebSocket varint-header framing (+ tests)
│  │     ├─ instance_uid.rs          16-byte InstanceUid newtype
│  │     ├─ capabilities.rs          capability bitmask helpers
│  │     ├─ message.rs               config_hash + AgentConfigMap builder
│  │     ├─ event.rs                 OpampEvent
│  │     ├─ error.rs                 OpampError
│  │     └─ server/{mod,ws,registry}.rs   the embedded OpAMP server
│  ├─ otelc-otlp/                    embedded OTLP receiver + telemetry store
│  │  └─ src/{lib,receiver,store,model}.rs
│  └─ otelc-tui/                     the binary
│     └─ src/
│        ├─ main.rs                  CLI, logging, mode selection, startup
│        ├─ cli.rs / config.rs       flags and optional YAML config
│        ├─ control/{mod,embedded,external}.rs   ControlPlane abstraction
│        ├─ pipeline.rs              effective-config → pipeline graph (+ tests)
│        ├─ app.rs                   App state, event loop, key handling
│        ├─ theme.rs                 Norton Commander palette
│        ├─ ui.rs                    root render: bars, menu, modals
│        └─ views.rs                 the six panel view renderers
└─ examples/mock-agent/              simulated collector fleet for testing
```

## 4. Embedded OpAMP server (`otelc-opamp`)

### Transport and framing

OpAMP messages travel as WebSocket **binary** frames: a varint-encoded header
(currently always `0`) followed by the protobuf payload. `framing::encode`
prepends the header; `framing::decode` consumes the varint regardless of value
(so a future non-zero header degrades to "skip the header") and decodes the
remainder. This is the highest-risk spec detail and is isolated with unit tests
including a hand-built byte vector.

### Connection lifecycle

`OpampServer::start` binds a `TcpListener` and spawns an accept loop; each
connection gets a task running `server::ws::serve`:

1. Upgrade to WebSocket (`tokio_tungstenite::accept_async`).
2. `select!` over inbound frames and an outbound push channel.
3. On the first `AgentToServer`, learn the 16-byte `instance_uid`, register the
   agent's push-channel sender in the `Registry`, and emit nothing extra — every
   message is surfaced as `OpampEvent::Message`.
4. Every `AgentToServer` gets a `ServerToAgent` reply carrying `instance_uid`
   and the server capability bitmask. The **first** reply also carries an
   OwnTelemetry `ConnectionSettingsOffers` if the agent advertises any
   `ReportsOwn{Metrics,Logs,Traces}` capability.
5. On socket close or `AgentDisconnect`, deregister and emit
   `OpampEvent::Disconnected`.

### Registry and the server handle

The `Registry` is a `Mutex<HashMap<InstanceUid, mpsc::Sender<ServerToAgent>>>`
— purely a routing table. All agent *state* lives in the control plane, not the
server. The cloneable `ServerHandle` exposes `offer_remote_config` (computes the
SHA-256 `config_hash`, returns it, pushes an `AgentRemoteConfig`) and
`send_restart` (pushes a `ServerToAgentCommand{Restart}`). Both look up the
agent's push channel and `try_send`; absence yields `SendError::NotConnected`.

## 5. OpAMP message flows

```
Connect / status
  agent ──AgentToServer{uid,caps,description,health,effective_config}──▶ server
  server ──ServerToAgent{uid,caps, [connection_settings on first reply]}──▶ agent

Remote-config push
  otelc  ──ServerToAgent{remote_config:{config, config_hash}}──▶ agent
  agent  ──AgentToServer{remote_config_status: APPLYING}──▶ server
  agent  ──AgentToServer{remote_config_status: APPLIED, effective_config}──▶ server

Restart
  otelc  ──ServerToAgent{command: Restart}──▶ agent
  agent  ──AgentToServer{health: new start_time}──▶ server

OwnTelemetry
  server ──ServerToAgent{connection_settings.own_metrics/logs/traces}──▶ agent
  agent  ──OTLP/gRPC ResourceMetrics/Logs/Spans──▶ otelc OTLP receiver
```

## 6. The `ControlPlane` abstraction (`otelc-tui/src/control`)

`ControlPlane` is the seam between the UI and the backends:

```rust
trait ControlPlane: Send + Sync {
    fn mode(&self) -> &'static str;
    fn endpoint(&self) -> String;
    fn push_config(&self, uid: &str, yaml: &str) -> Result<(), String>;
    fn restart(&self, uid: &str) -> Result<(), String>;
}
```

Updates flow to the UI as `ControlEvent`s on an `mpsc` channel:
`AgentUpserted`, `AgentDisconnected`, `Telemetry`, `Notice`.

**`EmbeddedControlPlane`** owns the OpAMP server and OTLP receiver. A
`translate` task accumulates sparse `AgentToServer` messages into full
`AgentDetail` snapshots (OpAMP messages only carry *changed* fields, so the
control plane keeps the per-agent running state) and emits `AgentUpserted`. A
`poll_telemetry` task snapshots the OTLP store every 1.5 s. `push_config` and
`restart` map the UID string to an `InstanceUid` and call the `ServerHandle`.

**`ExternalControlPlane`** implements the same trait against an external OpAMP
server URL. Since OpAMP defines no standard server-to-console API, a real
deployment supplies a backend-specific adapter; this build ships the struct,
the mode selection, and an explanatory `Notice`. The adapter shape — an inner
`list_agents / get_agent / push_config / restart / stream_events` trait with
one concrete implementation per backend — is the intended extension point.

## 7. OTLP receiver and telemetry store (`otelc-otlp`)

`OtlpReceiver::start` builds a `tonic` server hosting the OTLP Metrics, Logs and
Trace services and returns a shared `TelemetryStore`. Each `export` call flattens
the OTLP payload into the store, keyed by the `service.instance.id` resource
attribute (fallback `service.name`).

`TelemetryStore` is a `Mutex<HashMap<String, AgentTelemetry>>`. `AgentTelemetry`
holds bounded `VecDeque` ring buffers — 4 000 metric points, 2 000 log records —
with O(1) push and oldest-first eviction, so memory cannot grow unbounded.
Metric extraction reads Gauge and Sum numeric data points; logs read
`severity_text`/`severity_number` and the `body` AnyValue; traces are counted.

## 8. Effective-config model and pipeline graph (`otelc-tui/src/pipeline.rs`)

`pipeline::parse` deserializes the effective-config YAML with `serde_yaml_ng`,
reading `service.pipelines` and the set of declared `connectors`. The result is
a `PipelineGraph` of `Pipeline { name, receivers, processors, exporters }`.

A node is a *connector* if its name is a declared connector. `PipelineGraph::
bridges` finds connector bridges: a connector that is an exporter of pipeline A
and a receiver of pipeline B yields an edge `A ▶ B`, which is what makes the
topology a DAG. `views::render_pipeline` draws each pipeline as a coloured
`receivers ──▶ processors ──▶ exporters` flow, marks connector nodes with `⇄`,
and lists the bridges. (A fuller layered box-drawing renderer is future work;
the design keeps parsing and rendering separate so it can be swapped in.)

## 9. TUI architecture (`otelc-tui/src/app.rs`, `ui.rs`, `views.rs`)

`App` holds the `ControlPlane`, the agent map and display order, telemetry
snapshots, the two `Panel`s, the active side, an optional `Modal` and `Menu`,
the filter string, and a status line. `app::run` calls `ratatui::init()`,
spawns the input thread, and runs the `select!` loop, redrawing after every
event and on the render tick; it always calls `ratatui::restore()` on exit.

Each `Panel` has an independent `ViewMode` — `Fleet | Config | Pipeline |
Metrics | Logs | Health`. The left panel defaults to Fleet, the right to
Config. `ui::draw` lays the screen out vertically — menu bar, body, mini-status,
command line, function-key bar — and splits the body 50/50 into two
double-bordered panels. `views::render_panel` dispatches to the six renderers.

Modals (`Help`, `Message`, `Confirm`, `Editor`) render last over a `Clear`
rect and capture all input while open. The `Editor` is a small line-based text
editor (insert, delete, newline, cursor motion) that follows the cursor with a
real terminal cursor via `Frame::set_cursor_position`; `F5` pushes the buffer
as a remote-config offer. The pull-down `Menu` is a modal popup.

Config-push and restart are gated on the agent's advertised capabilities
(`AcceptsRemoteConfig`, `AcceptsRestartCommand`): unsupported actions report a
status message instead of failing silently.

## 10. Norton Commander theme (`otelc-tui/src/theme.rs`)

Blue background; cyan double-line panel borders (white when the panel is
active); white body text; yellow accents; black-on-cyan menu bar, function-key
bar and selection highlight; green/red health dots; pipeline nodes coloured by
kind (receiver green, processor cyan, exporter magenta, connector yellow). All
colours are 16-colour ANSI for graceful degradation.

## 11. Keybindings

| Key | Action |
|-----|--------|
| `Tab` | switch active panel |
| `1`–`6` | set active panel view (Fleet/Config/Pipeline/Metrics/Logs/Health) |
| `↑`/`↓`, `k`/`j` | move fleet selection, or scroll the active view |
| `PgUp`/`PgDn` | scroll by a page |
| `Enter` | open the selected agent's config in the right panel |
| `F1` | help · `F2`/`F9` menu · `F3` cycle view |
| `F4`/`F5` | edit / push remote config · `F6` restart agent |
| `F7` | filter the fleet · `F8` jump to Health view |
| `F10` / `q` | quit |

## 12. Configuration file (`otelc-tui/src/config.rs`)

An optional YAML file (`--config`) supplies `listen`, `otlp_listen` and
`external_url`. Precedence is CLI flag → config file → built-in default. See
`otelc.example.yaml`.

## 13. Error handling and logging

`otelc-opamp` uses a typed `OpampError`; the binary uses `anyhow`. Malformed
OpAMP frames are logged and dropped rather than killing a connection. The TUI
owns stdout, so all `tracing` output goes to a log file (`--log-file`, default
`otelc.log`) via a non-blocking writer. Failed control actions surface as an
error modal.

## 14. Security (required hardening)

The prototype binds OpAMP and OTLP to loopback. For real deployments: serve
OpAMP over `wss://` with a TLS certificate; require mTLS or a bearer token
(OpAMP connection-settings headers) so only authorized agents connect; place
the OTLP receiver behind the same trust boundary. The embedded server is a
control plane — restart and config-push reach any connected agent — so network
exposure must be paired with authentication.

## 15. Testing strategy and roadmap

**Automated tests** (`cargo test --workspace`):
- `otelc-opamp`: WebSocket framing round-trip and a known byte vector;
  `tests/server_roundtrip.rs` starts the real server, connects a WebSocket
  client, and asserts the OwnTelemetry offer, a remote-config push (hash +
  body), and restart-command delivery.
- `otelc-otlp`: ring-buffer eviction bounds and severity mapping.
- `otelc-tui`: `pipeline.rs` parses single, multi-pipeline and connector-bridged
  configs into the expected graph.

**End-to-end, no external downloads**: run `otelc` and `mock-agent` (see the
README). `mock-agent` simulates a fleet of collectors — connecting over OpAMP,
reporting a realistic multi-pipeline effective config and health, applying
remote-config offers, honoring restart commands, and pushing OTLP own-telemetry.

**Optional, against the reference implementation**: point a real OpAMP
Supervisor at `ws://127.0.0.1:4320/v1/opamp` with `accepts_remote_config`
enabled.

**Roadmap**: plain-HTTP OpAMP transport (poll-based, with a per-UID pending-offer
queue); an effective-vs-proposed config diff in the editor; a layered
box-drawing pipeline renderer; a concrete external-server adapter.

## 16. Build and distribution

`cargo build --release` produces a single static-friendly binary per target.
The toolchain is pinned via `rust-toolchain.toml`. The `release` profile strips
symbols. macOS and Linux are supported; the only build-time external tool is
`protoc` for `prost-build` (the proto is vendored).
