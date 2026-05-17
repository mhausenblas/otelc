//! A mock OpAMP agent: simulates one or more OpenTelemetry Collectors so the
//! `otelc` TUI (or any OpAMP server) can be exercised end-to-end without
//! downloading a real collector or supervisor.
//!
//! Each simulated agent connects over the OpAMP WebSocket transport, reports a
//! realistic effective config, health and capabilities, applies remote-config
//! offers, honors restart commands, and pushes its own telemetry over OTLP.

use std::collections::HashMap;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use anyhow::Result;
use clap::Parser;
use futures_util::{SinkExt, StreamExt};
use otelc_opamp::framing::{decode, encode};
use otelc_opamp::{pb, InstanceUid};
use tokio_tungstenite::tungstenite::Message as WsMessage;
use tracing::{info, warn};

use opentelemetry_proto::tonic::collector::logs::v1::logs_service_client::LogsServiceClient;
use opentelemetry_proto::tonic::collector::logs::v1::ExportLogsServiceRequest;
use opentelemetry_proto::tonic::collector::metrics::v1::metrics_service_client::MetricsServiceClient;
use opentelemetry_proto::tonic::collector::metrics::v1::ExportMetricsServiceRequest;
use opentelemetry_proto::tonic::common::v1::{any_value, AnyValue, InstrumentationScope, KeyValue};
use opentelemetry_proto::tonic::logs::v1::{LogRecord, ResourceLogs, ScopeLogs};
use opentelemetry_proto::tonic::metrics::v1::{
    metric, number_data_point, AggregationTemporality, Gauge, Metric, NumberDataPoint,
    ResourceMetrics, ScopeMetrics, Sum,
};
use opentelemetry_proto::tonic::resource::v1::Resource;

const BASE_CONFIG: &str = r#"receivers:
  otlp:
    protocols:
      grpc:
        endpoint: 0.0.0.0:4317
      http:
        endpoint: 0.0.0.0:4318
  hostmetrics:
    collection_interval: 30s
processors:
  memory_limiter:
    check_interval: 1s
    limit_mib: 512
  resourcedetection:
    detectors: [env, system]
  batch:
    timeout: 5s
connectors:
  spanmetrics:
    histogram:
      explicit:
        buckets: [10ms, 100ms, 1s]
exporters:
  otlphttp:
    endpoint: https://backend.example.com:4318
  prometheus:
    endpoint: 0.0.0.0:8889
  debug:
    verbosity: basic
extensions:
  health_check:
    endpoint: 0.0.0.0:13133
  opamp:
    server:
      ws:
        endpoint: ws://127.0.0.1:4320/v1/opamp
service:
  extensions: [health_check, opamp]
  pipelines:
    traces:
      receivers: [otlp]
      processors: [memory_limiter, resourcedetection, batch]
      exporters: [otlphttp, spanmetrics]
    metrics:
      receivers: [otlp, hostmetrics, spanmetrics]
      processors: [memory_limiter, batch]
      exporters: [otlphttp, prometheus]
    logs:
      receivers: [otlp]
      processors: [memory_limiter, batch]
      exporters: [otlphttp, debug]
"#;

#[derive(Parser)]
#[command(about = "Mock OpAMP agent fleet for exercising the otelc TUI")]
struct Args {
    /// OpAMP server WebSocket URL.
    #[arg(long, default_value = "ws://127.0.0.1:4320/v1/opamp")]
    server: String,
    /// OTLP/gRPC endpoint for pushing own telemetry.
    #[arg(long, default_value = "http://127.0.0.1:4317")]
    otlp: String,
    /// Number of simulated agents.
    #[arg(long, default_value_t = 3)]
    count: usize,
}

#[derive(Clone)]
struct Profile {
    name: String,
    version: String,
    host: String,
    degraded: bool,
}

fn profiles(count: usize) -> Vec<Profile> {
    let templates = [
        ("otel-gateway-01", "0.110.0", "ip-10-0-1-12", false),
        ("otel-agent-fra", "0.110.0", "fra-edge-3", false),
        ("otel-edge-collector", "0.109.1", "pop-syd-1", true),
        ("otel-gateway-02", "0.110.0", "ip-10-0-2-44", false),
        ("otel-agent-iad", "0.110.0", "iad-edge-7", false),
    ];
    (0..count)
        .map(|i| {
            let (name, version, host, degraded) = templates[i % templates.len()];
            let suffix = if i >= templates.len() {
                format!("-{i}")
            } else {
                String::new()
            };
            Profile {
                name: format!("{name}{suffix}"),
                version: version.to_string(),
                host: format!("{host}{suffix}"),
                degraded,
            }
        })
        .collect()
}

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_target(false)
        .with_max_level(tracing::Level::INFO)
        .init();
    let args = Args::parse();
    info!(server = %args.server, count = args.count, "starting mock agent fleet");

    let mut handles = Vec::new();
    for (idx, profile) in profiles(args.count).into_iter().enumerate() {
        let server = args.server.clone();
        let otlp = args.otlp.clone();
        handles.push(tokio::spawn(async move {
            // Stagger connections slightly so the fleet appears gradually.
            tokio::time::sleep(Duration::from_millis(idx as u64 * 400)).await;
            loop {
                if let Err(e) = run_agent(&profile, &server, &otlp).await {
                    warn!(agent = %profile.name, error = %e, "agent disconnected, reconnecting");
                }
                tokio::time::sleep(Duration::from_secs(3)).await;
            }
        }));
    }
    for handle in handles {
        let _ = handle.await;
    }
    Ok(())
}

async fn run_agent(profile: &Profile, server: &str, otlp: &str) -> Result<()> {
    let uid = InstanceUid::new_v7();
    let uid_str = uid.to_string();
    let (ws, _) = tokio_tungstenite::connect_async(server).await?;
    let (mut write, mut read) = ws.split();
    info!(agent = %profile.name, uid = %uid, "connected to OpAMP server");

    let mut start_ns = now_ns();
    let mut seq = 0u64;
    let mut effective = BASE_CONFIG.to_string();

    seq += 1;
    let first = full_status(&uid, seq, profile, &effective, start_ns);
    write.send(WsMessage::Binary(encode(&first).into())).await?;

    let mut metrics_client: Option<MetricsServiceClient<tonic::transport::Channel>> = None;
    let mut logs_client: Option<LogsServiceClient<tonic::transport::Channel>> = None;
    let mut ticker = tokio::time::interval(Duration::from_secs(3));
    ticker.tick().await; // consume the immediate first tick
    let mut tick = 0u64;

    loop {
        tokio::select! {
            incoming = read.next() => {
                match incoming {
                    Some(Ok(WsMessage::Binary(data))) => {
                        let s2a: pb::ServerToAgent = decode(&data)?;
                        if let Some(rc) = s2a.remote_config {
                            if let Some(map) = &rc.config {
                                if let Some(file) = map.config_map.get("")
                                    .or_else(|| map.config_map.values().next())
                                {
                                    effective = String::from_utf8_lossy(&file.body).to_string();
                                }
                            }
                            info!(agent = %profile.name, "received remote config offer");
                            seq += 1;
                            write.send(WsMessage::Binary(
                                encode(&config_status(&uid, seq, &rc.config_hash,
                                    pb::RemoteConfigStatuses::Applying, "")).into())).await?;
                            tokio::time::sleep(Duration::from_millis(700)).await;
                            seq += 1;
                            write.send(WsMessage::Binary(
                                encode(&config_applied(&uid, seq, &rc.config_hash, &effective)).into())).await?;
                            info!(agent = %profile.name, "applied remote config");
                        }
                        if let Some(cmd) = s2a.command {
                            if cmd.r#type == pb::CommandType::Restart as i32 {
                                info!(agent = %profile.name, "restart command received");
                                start_ns = now_ns();
                                seq += 1;
                                write.send(WsMessage::Binary(
                                    encode(&health_report(&uid, seq, profile, start_ns)).into())).await?;
                            }
                        }
                        if let Some(cs) = s2a.connection_settings {
                            if let Some(own) = cs.own_metrics {
                                info!(agent = %profile.name, endpoint = %own.destination_endpoint,
                                    "received own-telemetry offer");
                            }
                        }
                    }
                    Some(Ok(WsMessage::Ping(p))) => { write.send(WsMessage::Pong(p)).await?; }
                    Some(Ok(WsMessage::Close(_))) | None => break,
                    Some(Err(e)) => return Err(e.into()),
                    _ => {}
                }
            }
            _ = ticker.tick() => {
                tick += 1;
                seq += 1;
                write.send(WsMessage::Binary(
                    encode(&health_report(&uid, seq, profile, start_ns)).into())).await?;
                push_metrics(otlp, &uid_str, profile, tick, &mut metrics_client).await;
                push_logs(otlp, &uid_str, profile, tick, &mut logs_client).await;
            }
        }
    }
    Ok(())
}

// --- OpAMP message builders -------------------------------------------------

fn agent_caps() -> u64 {
    use pb::AgentCapabilities::*;
    ReportsStatus as u64
        | AcceptsRemoteConfig as u64
        | ReportsEffectiveConfig as u64
        | ReportsHealth as u64
        | ReportsRemoteConfig as u64
        | AcceptsRestartCommand as u64
        | ReportsOwnMetrics as u64
        | ReportsOwnLogs as u64
        | AcceptsOpAmpConnectionSettings as u64
}

fn full_status(
    uid: &InstanceUid,
    seq: u64,
    profile: &Profile,
    effective: &str,
    start_ns: u64,
) -> pb::AgentToServer {
    pb::AgentToServer {
        instance_uid: uid.to_vec(),
        sequence_num: seq,
        capabilities: agent_caps(),
        agent_description: Some(agent_description(uid, profile)),
        health: Some(health(profile, start_ns)),
        effective_config: Some(effective_config(effective)),
        ..Default::default()
    }
}

fn health_report(
    uid: &InstanceUid,
    seq: u64,
    profile: &Profile,
    start_ns: u64,
) -> pb::AgentToServer {
    pb::AgentToServer {
        instance_uid: uid.to_vec(),
        sequence_num: seq,
        capabilities: agent_caps(),
        health: Some(health(profile, start_ns)),
        ..Default::default()
    }
}

fn config_status(
    uid: &InstanceUid,
    seq: u64,
    hash: &[u8],
    status: pb::RemoteConfigStatuses,
    error: &str,
) -> pb::AgentToServer {
    pb::AgentToServer {
        instance_uid: uid.to_vec(),
        sequence_num: seq,
        capabilities: agent_caps(),
        remote_config_status: Some(pb::RemoteConfigStatus {
            last_remote_config_hash: hash.to_vec(),
            status: status as i32,
            error_message: error.to_string(),
        }),
        ..Default::default()
    }
}

fn config_applied(uid: &InstanceUid, seq: u64, hash: &[u8], effective: &str) -> pb::AgentToServer {
    let mut msg = config_status(uid, seq, hash, pb::RemoteConfigStatuses::Applied, "");
    msg.effective_config = Some(effective_config(effective));
    msg
}

fn effective_config(yaml: &str) -> pb::EffectiveConfig {
    pb::EffectiveConfig {
        config_map: Some(otelc_opamp::message::yaml_config_map(yaml)),
    }
}

fn agent_description(uid: &InstanceUid, profile: &Profile) -> pb::AgentDescription {
    pb::AgentDescription {
        identifying_attributes: vec![
            opamp_kv("service.name", "io.opentelemetry.collector"),
            opamp_kv("service.version", &profile.version),
            opamp_kv("service.instance.id", &uid.to_string()),
        ],
        non_identifying_attributes: vec![
            opamp_kv("os.type", std::env::consts::OS),
            opamp_kv("host.name", &profile.host),
            opamp_kv("host.arch", std::env::consts::ARCH),
        ],
    }
}

fn health(profile: &Profile, start_ns: u64) -> pb::ComponentHealth {
    let mut sub = HashMap::new();
    sub.insert(
        "pipeline:traces".to_string(),
        sub_health(true, "StatusOK", ""),
    );
    sub.insert(
        "pipeline:metrics".to_string(),
        sub_health(true, "StatusOK", ""),
    );
    sub.insert(
        "pipeline:logs".to_string(),
        sub_health(
            !profile.degraded,
            if profile.degraded {
                "StatusRecoverableError"
            } else {
                "StatusOK"
            },
            if profile.degraded {
                "exporter otlphttp: connection refused"
            } else {
                ""
            },
        ),
    );
    sub.insert(
        "extension:health_check".to_string(),
        sub_health(true, "StatusOK", ""),
    );
    pb::ComponentHealth {
        healthy: !profile.degraded,
        start_time_unix_nano: start_ns,
        last_error: if profile.degraded {
            "1 pipeline reporting recoverable errors".to_string()
        } else {
            String::new()
        },
        status: if profile.degraded {
            "StatusRecoverableError"
        } else {
            "StatusOK"
        }
        .to_string(),
        status_time_unix_nano: now_ns(),
        component_health_map: sub,
    }
}

fn sub_health(healthy: bool, status: &str, error: &str) -> pb::ComponentHealth {
    pb::ComponentHealth {
        healthy,
        start_time_unix_nano: 0,
        last_error: error.to_string(),
        status: status.to_string(),
        status_time_unix_nano: now_ns(),
        component_health_map: HashMap::new(),
    }
}

fn opamp_kv(key: &str, value: &str) -> pb::KeyValue {
    pb::KeyValue {
        key: key.to_string(),
        value: Some(pb::AnyValue {
            value: Some(pb::any_value::Value::StringValue(value.to_string())),
        }),
    }
}

// --- OTLP own-telemetry -----------------------------------------------------

async fn push_metrics(
    otlp: &str,
    uid: &str,
    profile: &Profile,
    tick: u64,
    client: &mut Option<MetricsServiceClient<tonic::transport::Channel>>,
) {
    if client.is_none() {
        match MetricsServiceClient::connect(otlp.to_string()).await {
            Ok(c) => *client = Some(c),
            Err(e) => {
                warn!(agent = %profile.name, error = %e, "OTLP metrics connect failed");
                return;
            }
        }
    }
    let request = build_metrics(uid, profile, tick);
    if let Some(c) = client.as_mut() {
        if let Err(e) = c.export(request).await {
            warn!(agent = %profile.name, error = %e, "OTLP metrics export failed");
            *client = None;
        }
    }
}

async fn push_logs(
    otlp: &str,
    uid: &str,
    profile: &Profile,
    tick: u64,
    client: &mut Option<LogsServiceClient<tonic::transport::Channel>>,
) {
    if client.is_none() {
        match LogsServiceClient::connect(otlp.to_string()).await {
            Ok(c) => *client = Some(c),
            Err(e) => {
                warn!(agent = %profile.name, error = %e, "OTLP logs connect failed");
                return;
            }
        }
    }
    let request = build_logs(uid, profile, tick);
    if let Some(c) = client.as_mut() {
        if let Err(e) = c.export(request).await {
            warn!(agent = %profile.name, error = %e, "OTLP logs export failed");
            *client = None;
        }
    }
}

fn resource(uid: &str) -> Resource {
    Resource {
        attributes: vec![
            otlp_kv("service.name", "io.opentelemetry.collector"),
            otlp_kv("service.instance.id", uid),
        ],
        ..Default::default()
    }
}

fn build_metrics(uid: &str, profile: &Profile, tick: u64) -> ExportMetricsServiceRequest {
    let queue = if profile.degraded {
        80 + tick * 5
    } else {
        4 + tick % 6
    };
    let metrics = vec![
        sum_metric("otelcol_process_uptime", "s", (tick * 3) as f64),
        gauge_metric(
            "otelcol_process_memory_rss",
            "By",
            1.05e8 + (tick % 11) as f64 * 2.0e6,
        ),
        sum_metric("otelcol_receiver_accepted_spans", "1", (tick * 1840) as f64),
        sum_metric("otelcol_exporter_sent_spans", "1", (tick * 1820) as f64),
        gauge_metric("otelcol_exporter_queue_size", "1", queue as f64),
        gauge_metric("otelcol_processor_batch_batch_send_size", "1", 512.0),
    ];
    ExportMetricsServiceRequest {
        resource_metrics: vec![ResourceMetrics {
            resource: Some(resource(uid)),
            scope_metrics: vec![ScopeMetrics {
                scope: Some(InstrumentationScope {
                    name: "go.opentelemetry.io/collector/service".to_string(),
                    ..Default::default()
                }),
                metrics,
                schema_url: String::new(),
            }],
            schema_url: String::new(),
        }],
    }
}

fn build_logs(uid: &str, profile: &Profile, tick: u64) -> ExportLogsServiceRequest {
    let mut records = vec![log_record(
        9,
        "INFO",
        "Everything is ready. Begin running and processing data.",
    )];
    if profile.degraded {
        records.push(log_record(
            17,
            "ERROR",
            "Exporting failed. Will retry the request after interval. {exporter=otlphttp}",
        ));
    } else if tick.is_multiple_of(4) {
        records.push(log_record(
            13,
            "WARN",
            "Memory usage is above soft limit, forcing a GC.",
        ));
    }
    ExportLogsServiceRequest {
        resource_logs: vec![ResourceLogs {
            resource: Some(resource(uid)),
            scope_logs: vec![ScopeLogs {
                scope: Some(InstrumentationScope {
                    name: "go.opentelemetry.io/collector/service".to_string(),
                    ..Default::default()
                }),
                log_records: records,
                schema_url: String::new(),
            }],
            schema_url: String::new(),
        }],
    }
}

fn gauge_metric(name: &str, unit: &str, value: f64) -> Metric {
    Metric {
        name: name.to_string(),
        unit: unit.to_string(),
        data: Some(metric::Data::Gauge(Gauge {
            data_points: vec![number_point(value)],
        })),
        ..Default::default()
    }
}

fn sum_metric(name: &str, unit: &str, value: f64) -> Metric {
    Metric {
        name: name.to_string(),
        unit: unit.to_string(),
        data: Some(metric::Data::Sum(Sum {
            data_points: vec![number_point(value)],
            aggregation_temporality: AggregationTemporality::Cumulative as i32,
            is_monotonic: true,
        })),
        ..Default::default()
    }
}

fn number_point(value: f64) -> NumberDataPoint {
    NumberDataPoint {
        time_unix_nano: now_ns(),
        value: Some(number_data_point::Value::AsDouble(value)),
        ..Default::default()
    }
}

fn log_record(severity_number: i32, severity_text: &str, body: &str) -> LogRecord {
    LogRecord {
        time_unix_nano: now_ns(),
        observed_time_unix_nano: now_ns(),
        severity_number,
        severity_text: severity_text.to_string(),
        body: Some(AnyValue {
            value: Some(any_value::Value::StringValue(body.to_string())),
        }),
        ..Default::default()
    }
}

fn otlp_kv(key: &str, value: &str) -> KeyValue {
    KeyValue {
        key: key.to_string(),
        value: Some(AnyValue {
            value: Some(any_value::Value::StringValue(value.to_string())),
        }),
        ..Default::default()
    }
}

fn now_ns() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos() as u64
}
