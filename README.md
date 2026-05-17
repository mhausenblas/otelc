# otelc

A **Norton Commander-style terminal UI** for managing fleets of
**OpenTelemetry Collectors** over the **Open Agent Management Protocol
(OpAMP)** — built for infrastructure and platform engineers.

`otelc` unifies what is normally several disconnected tools into one fast,
keyboard-driven, terminal-native console: view and push collector
configuration, watch each collector's own metrics and logs, introspect agent
identity / capabilities / health, and visualize pipeline topology — all over
OpAMP, with no external backend required.

```
 otelc  Fleet Config Pipeline Metrics Logs Health  │  embedded ws://127.0.0.1:4320/v1/opamp · 3 agent(s)
╔═ Fleet · 3 agent(s) ════════════════╗╔═ Pipeline · otel-gateway-01 ═════════════╗
║ STATE NAME            VERSION STATUS ║║▣ traces                                  ║
║●up   otel-agent-fra   0.110.0 Healthy║║   otlp ──▶ memory_limiter ▸ batch ──▶    ║
║ otel-edge-collector  0.109.1 Recover ║║   otlphttp · spanmetrics⇄                ║
║ otel-gateway-01      0.110.0 Healthy ║║▣ metrics                                 ║
║                                      ║║   otlp · spanmetrics⇄ ──▶ batch ──▶ …    ║
╚══════════════════════════════════════╝╚══════════════════════════════════════════╝
 1Help 2Menu 3View 4Edit 5Push 6Restart 7Filter 8Health 9Menu 10Quit
```

See [`docs/design/mockups.md`](docs/design/mockups.md) for all screens.

## Status

Working prototype. The embedded OpAMP server, the OTLP receiver, the full
Norton Commander TUI, config push, restart, and the pipeline graph all work
end-to-end. See [`docs/design/`](docs/design/) for the design and the roadmap
(plain-HTTP OpAMP transport, config diff, TLS/mTLS, a concrete external-server
adapter).

## Build

Requires a stable Rust toolchain and `protoc` (the protobuf compiler, used by
`prost-build`; the OpAMP `.proto` itself is vendored under `proto/`).

```sh
# Debian/Ubuntu: apt-get install -y protobuf-compiler
# macOS:         brew install protobuf
cargo build --release
```

## Run

`otelc` runs an **embedded OpAMP server** by default. Collectors — or their
OpAMP supervisors — connect to it.

```sh
otelc                               # OpAMP on ws://127.0.0.1:4320, OTLP on :4317
otelc --listen 0.0.0.0:4320 --otlp-listen 0.0.0.0:4317
otelc --config otelc.example.yaml
otelc --mode external --external-url http://my-opamp-server:8080
```

## Try it without a real collector

The `mock-agent` example simulates a fleet of OpenTelemetry Collectors — they
connect over OpAMP, report a realistic multi-pipeline config and health, apply
remote-config offers, honor restart commands, and push OTLP own-telemetry.

In one terminal:

```sh
cargo run --release --bin otelc
```

In another:

```sh
cargo run --release --bin mock-agent -- --count 3
```

The three agents appear in the Fleet panel. Press `Tab` to focus the right
panel, `1`–`6` to switch its view (Fleet / Config / Pipeline / Metrics / Logs /
Health), `F4` to edit an agent's config and `F5` to push it, `F6` to restart an
agent, `F1` for help, `F10` to quit.

## Use a real OpAMP Supervisor

Point an [OpAMP Supervisor](https://github.com/open-telemetry/opentelemetry-collector-contrib/tree/main/cmd/opampsupervisor)
at the embedded server by setting, in `supervisor.yaml`:

```yaml
server:
  endpoint: ws://127.0.0.1:4320/v1/opamp
capabilities:
  accepts_remote_config: true
  reports_own_metrics: true
  reports_own_logs: true
```

## Project layout

```
crates/otelc-opamp   OpAMP wire types, WebSocket framing, embedded OpAMP server
crates/otelc-otlp    Embedded OTLP/gRPC receiver + bounded telemetry store
crates/otelc-tui     The `otelc` binary: Norton Commander TUI + ControlPlane
examples/mock-agent  Simulated collector fleet for end-to-end testing
proto/               Vendored OpAMP protobuf schema
docs/design/         High-level design, detailed design, UI mockups
```

## Test

```sh
cargo test --workspace      # framing, OpAMP server round-trip, store, pipeline parser
cargo clippy --workspace
```

## License

MIT — see [LICENSE](LICENSE).
