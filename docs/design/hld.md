# otelc — High-Level Design

## 1. Overview

`otelc` is a single-binary terminal UI, styled after the classic **Norton
Commander** file manager, for managing fleets of **OpenTelemetry Collectors**
over the **Open Agent Management Protocol (OpAMP)**.

It is built for infrastructure and platform engineers who run many collectors
and today juggle disconnected tools: a YAML editor for config, `curl` against
`:8888`/zPages for introspection, a separate metrics backend to check whether
the collectors themselves are healthy, and OpAMP backends that are mostly web
UIs. `otelc` unifies *configuration + control + telemetry + topology* into one
fast, keyboard-driven, terminal-native tool that runs on any VT100-class
terminal on macOS and Linux.

### Goals

- View and edit collector configuration and push it back over OpAMP.
- Show each collector's own metrics and logs.
- Introspect agents: identity, advertised capabilities, component health.
- Visualize the receiver → processor → exporter → connector pipeline topology.
- Work against a self-contained embedded OpAMP server, with no external backend.
- Run anywhere a terminal does; degrade gracefully on basic terminals.

### Non-goals

- Not a replacement for a long-term metrics/observability backend — `otelc`
  shows *live* collector self-telemetry, not historical data.
- Not a general OpAMP backend/control system — it is an operator console.
- Does not manage agent *packages* (the OpAMP package-management capability).

## 2. Primer: OpAMP and the OpenTelemetry Collector

The **OpenTelemetry Collector** is a pipeline engine: *receivers* accept
telemetry, *processors* transform it, *exporters* send it onward, and
*connectors* bridge one pipeline's output into another pipeline's input.

**OpAMP** is a client/server protocol for remote agent management. The agent
(collector, or its supervisor) is the OpAMP *client*; the management plane is
the OpAMP *server*. OpAMP carries: agent description and capabilities, health,
the effective configuration, remote-config offers and their apply status,
restart commands, and *connection settings* — including where an agent should
send its **own telemetry**.

OpAMP is typically spoken by the **OpAMP Supervisor**, a process that wraps the
collector, applies remote config (by rewriting the config file and restarting
the collector) and reports the collector's own telemetry. The in-collector
`opamp` extension is read-only by comparison: it reports status and effective
config but cannot apply remote config.

## 3. Target user and use cases

The user is an infrastructure / platform engineer operating a collector fleet.

| Use case | How `otelc` serves it |
|----------|----------------------|
| Fleet triage | The Fleet panel lists every connected agent with health, version and last-seen at a glance. |
| Config review | The Config view renders an agent's effective config; the Pipeline view renders its topology. |
| Config rollout | Edit remote config in-place and push it; watch `RemoteConfigStatus` go `APPLYING → APPLIED`. |
| Incident debug | Metrics and Logs views show the collector's *own* telemetry; the Health view shows the component-health tree. |
| Capability check | The Health view lists each agent's advertised OpAMP capabilities, so unsupported actions are visible. |

## 4. System context

```
        ┌────────────────────── otelc (single binary) ───────────────────────┐
        │                                                                    │
        │   Norton Commander TUI  ◀──ControlEvent──  ControlPlane             │
        │          │                                  │        │             │
        │     key/command                     EmbeddedControlPlane            │
        │          ▼                              │         │                │
        │   ┌──────────────┐   ┌──────────────────┴──┐  ┌────┴─────────────┐  │
        │   │ panels/views │   │ embedded OpAMP server│  │ embedded OTLP rx │  │
        │   └──────────────┘   └──────────┬───────────┘  └────────┬─────────┘  │
        └──────────────────────────────── │ ─────────────────────│───────────┘
                                  OpAMP   │  WebSocket    OTLP/gRPC│
                                          ▼                       ▼
                            ┌──────────────────────┐   own metrics / logs / traces
                            │ OpAMP Supervisor      │───────────────┘
                            │   └─ OTel Collector   │
                            └──────────────────────┘   (one per managed agent)
```

Collectors (via their supervisor) connect *into* `otelc`'s embedded OpAMP
server. `otelc` offers each agent an OTLP endpoint — its own embedded receiver —
so the collector's self-telemetry flows back for the Metrics and Logs views.

## 5. Architecture overview

`otelc` is a Cargo workspace of three library crates plus the binary and a test
fixture:

| Component | Crate | Responsibility |
|-----------|-------|----------------|
| OpAMP layer | `otelc-opamp` | OpAMP wire types, WebSocket framing, the embedded OpAMP server. |
| Telemetry layer | `otelc-otlp` | Embedded OTLP/gRPC receiver and a bounded telemetry store. |
| Application | `otelc-tui` (binary `otelc`) | The Norton Commander TUI, the `ControlPlane` abstraction, the pipeline parser. |
| Test fixture | `mock-agent` | A simulated collector fleet for end-to-end testing without external downloads. |

The UI never touches OpAMP or OTLP types. It talks to a `ControlPlane` trait and
consumes a stream of `ControlEvent`s. This decouples the UI from *how* agents
are reached and makes the two operating modes interchangeable.

## 6. Operating modes

- **Embedded** (default) — `otelc` *is* the OpAMP server. Collectors/supervisors
  connect directly to it. Self-contained: no external backend, ideal for local
  fleets, dev/test, and demos.
- **External** — `otelc` is a console onto a third-party OpAMP server. Because
  OpAMP standardizes only the Agent↔Server protocol and **not** a
  Server↔console API, external mode is an *adapter*: a concrete adapter must be
  written per backend. This build ships the `ControlPlane` abstraction and the
  mode selection; a backend-specific adapter is a documented extension point
  (see `dld.md` §6).

## 7. Feature areas

- **Configuration** — render the OpAMP *effective config*; edit it in an inline
  editor; push it as an OpAMP remote-config offer; surface `RemoteConfigStatus`.
- **Telemetry** — an embedded OTLP receiver collects each agent's own metrics
  and logs (OpAMP-native: agents are told where to send via OwnTelemetry
  connection settings). Documented fallback: scrape the collector's `:8888`
  Prometheus endpoint or zPages.
- **Introspection** — agent identity (identifying / non-identifying
  attributes), the advertised OpAMP capability set, and the recursive
  component-health tree.
- **Pipeline visualization** — the topology is parsed from the effective config
  YAML and drawn as receiver → processor → exporter flows, with connector
  bridges between pipelines highlighted.

## 8. UX philosophy: why Norton Commander

Norton Commander's twin-panel layout maps naturally onto fleet management: the
left panel is the **fleet**, the right panel is a **detail view** of the
selected agent. Either panel can switch view-mode — the NC "brief / full / tree
/ info" idea — so an operator can, for example, keep the fleet on the left and
a pipeline graph on the right. The function-key bar gives discoverable,
single-keystroke actions; everything is keyboard-first and works over SSH.

## 9. Cross-platform and terminal compatibility

`otelc` uses `ratatui` + `crossterm`, which target any VT100-class terminal on
macOS and Linux. It renders inside an 80×24 terminal and scales up. Colours are
the 16-colour ANSI set, so they degrade gracefully on a basic `TERM`. The binary
links no platform-specific services.

## 10. Security considerations

- OpAMP runs over WebSocket; production deployments should use `wss://` (TLS),
  and OpAMP supports mTLS and bearer tokens via connection-settings headers.
- The embedded OpAMP server is a control plane: anything that can connect can
  receive config offers and restart commands. It binds to `127.0.0.1` by
  default; exposing it more widely requires TLS and an authentication layer.
- The OTLP receiver accepts unauthenticated telemetry on a loopback port by
  default; the same network-exposure caveat applies.
- The prototype implements the loopback-by-default posture; TLS/mTLS and
  auth are detailed as required hardening in `dld.md` §14.

## 11. Risks, assumptions, open questions

- **No mature Rust OpAMP server library.** Mitigated by generating wire types
  from the vendored `opamp.proto` and implementing the (small, well-specified)
  server surface directly.
- **No standard OpAMP server-to-console API.** External mode is therefore
  honestly an adapter; embedded mode is the fully-functional path.
- **OpAMP UID ↔ OTLP resource correlation** is best-effort: telemetry is keyed
  by the `service.instance.id` resource attribute, which an agent is expected
  to set equal to its OpAMP instance UID.
- **Plain-HTTP OpAMP transport** is not in the prototype (WebSocket only);
  see `dld.md` §15 for the roadmap.

## 12. Roadmap

1. **Prototype (this build)** — embedded OpAMP server (WebSocket), OTLP
   receiver, the full NC TUI, config push, restart, pipeline graph, mock-agent.
2. **MVP** — plain-HTTP OpAMP transport; effective-vs-proposed config diff;
   TLS/mTLS; persistence of fleet history; a real external-server adapter.
3. **Beyond** — package management, multi-server federation, alerting hooks,
   saved config templates, an audit log of pushed changes.
