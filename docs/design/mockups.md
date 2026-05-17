# otelc — UI Mockups

Norton Commander-style screens. These are faithful to what the prototype
renders; colour is indicated in the legend since this document is monochrome.

## Legend

```
blue background        cyan double-line borders      white body text
yellow  accents / headers / keys      black-on-cyan  bars + selection
● green healthy   ● red degraded   ○ offline
pipeline nodes: receiver=green  processor=cyan  exporter=magenta  connector=yellow ⇄
```

Screen chrome, top to bottom: menu bar · two panels · mini-status line ·
command line · function-key bar.

## 1. Main view — Fleet (left) + Config (right)

```
 otelc  Fleet Config Pipeline Metrics Logs Health  │  embedded ws://127.0.0.1:4320/v1/opamp · 3 agent(s)
╔═ Fleet · 3 agent(s) ════════════════╗╔═ Config · otel-gateway-01 ═══════════════╗
║ STATE NAME            VERSION STATUS ║║receivers:                                ║
║●up   otel-agent-fra   0.110.0 Healthy║║  otlp:                                   ║
║ otel-edge-collector  0.109.1 Recover ║║    protocols:                            ║
║ otel-gateway-01      0.110.0 Healthy ║║      grpc:                               ║
║                                      ║║        endpoint: 0.0.0.0:4317            ║
║   ↑ selection bar (black-on-cyan) ↑  ║║      http:                               ║
║                                      ║║        endpoint: 0.0.0.0:4318            ║
║                                      ║║  hostmetrics:                            ║
║                                      ║║    collection_interval: 30s              ║
║                                      ║║processors:                               ║
║                                      ║║  memory_limiter: {check_interval: 1s}    ║
║                                      ║║  batch: {timeout: 5s}                    ║
╚══════════════════════════════════════╝╚══════════════════════════════════════════╝
 instance-uid 019e35c7-76ef-7e91-b2d6-09b8918dc2a5  health OK  caps STATUS·CFG·EFFCFG·HEALTH
 otelc> agent otel-gateway-01 connected
 1Help 2Menu 3View 4Edit 5Push 6Restart 7Filter 8Health 9Menu 10Quit
```

The active panel has a bright (white) border; the inactive panel's border is
cyan. The selected fleet row is a full-width black-on-cyan bar.

## 2. Pipeline visualization

The DAG is parsed from the selected agent's effective config. Connector nodes
are marked `⇄`; bridges between pipelines are listed below.

```
╔═ Fleet · 3 agent(s) ════════════════╗╔═ Pipeline · otel-gateway-01 ═════════════╗
║●up   otel-agent-fra   0.110.0 Healthy║║receivers  processors  exporters  connect⇄║
║ otel-edge-collector  0.109.1 Recover ║║                                          ║
║ otel-gateway-01      0.110.0 Healthy ║║▣ traces                                  ║
║                                      ║║   otlp ──▶ memory_limiter ▸ resourcede…  ║
║                                      ║║        ▸ batch ──▶ otlphttp · spanmetr.⇄ ║
║                                      ║║                                          ║
║                                      ║║▣ metrics                                 ║
║                                      ║║   otlp · hostmetrics · spanmetrics⇄ ──▶  ║
║                                      ║║   memory_limiter ▸ batch ──▶ otlphttp ·  ║
║                                      ║║   prometheus                             ║
║                                      ║║                                          ║
║                                      ║║▣ logs                                    ║
║                                      ║║   otlp ──▶ memory_limiter ▸ batch ──▶    ║
║                                      ║║   otlphttp · debug                       ║
║                                      ║║                                          ║
║                                      ║║connector bridges                         ║
║                                      ║║   spanmetrics  traces ▶ metrics          ║
╚══════════════════════════════════════╝╚══════════════════════════════════════════╝
 instance-uid 019e35c7-76ef-…  health OK  caps STATUS·CFG·EFFCFG·HEALTH·METRICS
 otelc> pipeline parsed: 3 pipelines, 1 connector bridge
 1Help 2Menu 3View 4Edit 5Push 6Restart 7Filter 8Health 9Menu 10Quit
```

## 3. Metrics view — collector own-telemetry

```
╔═ Fleet · 3 agent(s) ════════════════╗╔═ Metrics · otel-gateway-01 ══════════════╗
║●up   otel-agent-fra   0.110.0 Healthy║║ 6 metric series · 12 spans observed      ║
║ otel-edge-collector  0.109.1 Recover ║║                                          ║
║ otel-gateway-01      0.110.0 Healthy ║║  otelcol_exporter_queue_size          7  ║
║                                      ║║  otelcol_exporter_sent_spans      9.10k  ║
║                                      ║║  otelcol_process_memory_rss     105.0M  ║
║                                      ║║  otelcol_process_uptime              15  ║
║                                      ║║  otelcol_processor_batch_…          512  ║
║                                      ║║  otelcol_receiver_accepted_spans  9.20k  ║
║                                      ║║                                          ║
╚══════════════════════════════════════╝╚══════════════════════════════════════════╝
 instance-uid 019e35c7-76ef-…  health OK  caps STATUS·CFG·EFFCFG·HEALTH·METRICS·LOGS
 otelc> telemetry updated
 1Help 2Menu 3View 4Edit 5Push 6Restart 7Filter 8Health 9Menu 10Quit
```

## 4. Logs view — collector own-logs (newest first, severity coloured)

```
╔═ Fleet · 3 agent(s) ════════════════╗╔═ Logs · otel-edge-collector ═════════════╗
║ otel-agent-fra       0.110.0 Healthy ║║ 11:52:21 ERROR  Exporting failed. Will   ║
║●down otel-edge-collector 0.109.1 Rec.║║          retry the request after inter… ║
║ otel-gateway-01      0.110.0 Healthy ║║ 11:52:21 INFO   Everything is ready.     ║
║   ↑ selected: degraded agent ↑       ║║          Begin running and processing…  ║
║                                      ║║ 11:52:18 ERROR  Exporting failed. Will   ║
║                                      ║║          retry the request after inter… ║
║                                      ║║ 11:52:18 INFO   Everything is ready. …   ║
║                                      ║║ 11:52:15 ERROR  Exporting failed. …      ║
╚══════════════════════════════════════╝╚══════════════════════════════════════════╝
 instance-uid 019e35c7-7a0f-…  health DEGRADED  caps STATUS·CFG·EFFCFG·HEALTH·LOGS
 otelc> telemetry updated
 1Help 2Menu 3View 4Edit 5Push 6Restart 7Filter 8Health 9Menu 10Quit
```

## 5. Health / introspection view

```
╔═ Fleet · 3 agent(s) ════════════════╗╔═ Health · otel-edge-collector ═══════════╗
║ otel-agent-fra       0.110.0 Healthy ║║ Identity                                 ║
║●down otel-edge-collector 0.109.1 Rec.║║  service.name        io.opentelemetry.…  ║
║ otel-gateway-01      0.110.0 Healthy ║║  service.version     0.109.1             ║
║                                      ║║  host.name           pop-syd-1           ║
║                                      ║║  uptime              4m 12s              ║
║                                      ║║ Capabilities                             ║
║                                      ║║  [x] ReportsStatus                       ║
║                                      ║║  [x] AcceptsRemoteConfig                 ║
║                                      ║║  [x] ReportsHealth                       ║
║                                      ║║  [ ] ReportsOwnTraces                    ║
║                                      ║║ Component health                         ║
║                                      ║║  ● agent  [StatusRecoverableError]       ║
║                                      ║║    ● pipeline:logs  exporter otlphttp:…  ║
║                                      ║║    ● pipeline:metrics  [StatusOK]        ║
║                                      ║║    ● pipeline:traces   [StatusOK]        ║
╚══════════════════════════════════════╝╚══════════════════════════════════════════╝
 instance-uid 019e35c7-7a0f-…  health DEGRADED  caps STATUS·CFG·EFFCFG·HEALTH
 otelc> 1 pipeline reporting recoverable errors
 1Help 2Menu 3View 4Edit 5Push 6Restart 7Filter 8Health 9Menu 10Quit
```

## 6. Remote-config editor (F4) — push with F5

```
        ╔═ Edit remote config — otel-gateway-01 ════════════════════╗
        ║receivers:                                                 ║
        ║  otlp:                                                    ║
        ║    protocols:                                             ║
        ║      grpc:                                                ║
        ║        endpoint: 0.0.0.0:4317                             ║
        ║processors:                                                ║
        ║  batch:                                                   ║
        ║    timeout: 10s          ◀── edited, cursor here          ║
        ║exporters:                                                 ║
        ║  otlphttp:                                                ║
        ║    endpoint: https://backend.example.com:4318             ║
        ║service:                                                   ║
        ║  pipelines:                                               ║
        ║    traces: {receivers: [otlp], processors: [batch], …}    ║
        ║ F5 Push   Esc Cancel   arrows move   Enter newline        ║
        ╚═══════════════════════════════════════════════════════════╝
```

`F5` sends the buffer as an OpAMP remote-config offer; the mini-status then
shows the agent's `RemoteConfigStatus` transition `APPLYING → APPLIED`.

## 7. Pull-down menu (F2 / F9)

```
 otelc  Fleet Config Pipeline Metrics Logs Health  │  embedded ws://127.0.0.1:4320 · 3 agent(s)
 ╔═ Menu ═══════════════════════╗═══════╗╔═ Config · otel-gateway-01 ═══════════════╗
 ║ Fleet view          1        ║Healthy║║receivers:                                ║
 ║ Config view         2        ║Recover║║  otlp:                                   ║
 ║ Pipeline view       3        ║Healthy║║    protocols:                            ║
 ║ Metrics view        4        ║       ║║      grpc:                               ║
 ║ Logs view           5        ║       ║║        endpoint: 0.0.0.0:4317            ║
 ║ Health view         6        ║       ║║processors:                               ║
 ║──────────────────────────────║       ║║  batch: {timeout: 5s}                    ║
 ║ Edit remote config F4        ║       ║║exporters:                                ║
 ║ Restart agent      F6        ║       ║║  otlphttp: {endpoint: https://…}         ║
 ║ Filter fleet       F7        ║       ║║service:                                  ║
 ║──────────────────────────────║       ║║  pipelines: [traces, metrics, logs]      ║
 ║ Help               F1        ║       ║║                                          ║
 ║ Quit              F10        ║       ║║                                          ║
 ╚══════════════════════════════╝═══════╝╚══════════════════════════════════════════╝
 instance-uid 019e35c7-76ef-…  health OK  caps STATUS·CFG·EFFCFG·HEALTH
 otelc> ready
 1Help 2Menu 3View 4Edit 5Push 6Restart 7Filter 8Health 9Menu 10Quit
```

The highlighted menu row is a black-on-cyan bar; `↑`/`↓` move, `Enter` selects.

## 8. Confirmation dialog (F6 — restart agent)

```
              ╔═ Restart agent ════════════════════════════╗
              ║                                            ║
              ║  Send a restart command to otel-gateway-01?║
              ║                                            ║
              ║  [ Enter = Yes ]    [ Esc = No ]           ║
              ╚════════════════════════════════════════════╝
```

## 9. Help (F1)

```
        ╔═ Help — otelc ══════════════════════════════════════════╗
        ║                                                         ║
        ║  Tab        switch active panel                         ║
        ║  1-6        set active panel view                       ║
        ║  Up/Down    move fleet selection, or scroll a view      ║
        ║  Enter      open the selected agent's config            ║
        ║  F1         this help                                   ║
        ║  F2 / F9    open the menu                               ║
        ║  F3         cycle the active panel's view               ║
        ║  F4 / F5    edit and push remote config                 ║
        ║  F6         restart the selected agent                  ║
        ║  F7         filter the fleet                            ║
        ║  F10 / q    quit                                        ║
        ║                                                         ║
        ║  otelc manages OpenTelemetry Collectors over OpAMP.     ║
        ╚═════════════════════════════════════════════════════════╝
```

## 10. Filter mode (F7)

The command line becomes a filter input; the Fleet panel narrows live.

```
╔═ Fleet · 1 agent(s) ════════════════╗╔═ Config · otel-gateway-01 ═══════════════╗
║ STATE NAME            VERSION STATUS ║║receivers:                                ║
║●up   otel-gateway-01  0.110.0 Healthy║║  otlp: {protocols: {grpc: {endpoint: …}}}║
║                                      ║║processors:                               ║
║                                      ║║  batch: {timeout: 5s}                    ║
╚══════════════════════════════════════╝╚══════════════════════════════════════════╝
 instance-uid 019e35c7-76ef-…  health OK  caps STATUS·CFG·EFFCFG·HEALTH
 filter> gateway█
 1Help 2Menu 3View 4Edit 5Push 6Restart 7Filter 8Health 9Menu 10Quit
```
