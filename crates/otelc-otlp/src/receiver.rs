//! The OTLP/gRPC server that feeds the [`TelemetryStore`].

use crate::store::TelemetryStore;
use opentelemetry_proto::tonic::collector::logs::v1::logs_service_server::{
    LogsService, LogsServiceServer,
};
use opentelemetry_proto::tonic::collector::logs::v1::{
    ExportLogsServiceRequest, ExportLogsServiceResponse,
};
use opentelemetry_proto::tonic::collector::metrics::v1::metrics_service_server::{
    MetricsService, MetricsServiceServer,
};
use opentelemetry_proto::tonic::collector::metrics::v1::{
    ExportMetricsServiceRequest, ExportMetricsServiceResponse,
};
use opentelemetry_proto::tonic::collector::trace::v1::trace_service_server::{
    TraceService, TraceServiceServer,
};
use opentelemetry_proto::tonic::collector::trace::v1::{
    ExportTraceServiceRequest, ExportTraceServiceResponse,
};
use std::net::SocketAddr;
use std::sync::Arc;
use tonic::{Request, Response, Status};
use tracing::info;

/// The embedded OTLP receiver.
pub struct OtlpReceiver;

impl OtlpReceiver {
    /// Start the gRPC server on `addr` and return the shared telemetry store.
    pub async fn start(addr: SocketAddr) -> Result<Arc<TelemetryStore>, tonic::transport::Error> {
        let store = Arc::new(TelemetryStore::new());
        let metrics = MetricsServiceServer::new(MetricsEndpoint {
            store: store.clone(),
        });
        let logs = LogsServiceServer::new(LogsEndpoint {
            store: store.clone(),
        });
        let traces = TraceServiceServer::new(TraceEndpoint {
            store: store.clone(),
        });

        tokio::spawn(async move {
            info!(%addr, "OTLP receiver listening");
            let result = tonic::transport::Server::builder()
                .add_service(metrics)
                .add_service(logs)
                .add_service(traces)
                .serve(addr)
                .await;
            if let Err(e) = result {
                tracing::error!(error = %e, "OTLP receiver stopped");
            }
        });

        Ok(store)
    }
}

struct MetricsEndpoint {
    store: Arc<TelemetryStore>,
}

#[tonic::async_trait]
impl MetricsService for MetricsEndpoint {
    async fn export(
        &self,
        request: Request<ExportMetricsServiceRequest>,
    ) -> Result<Response<ExportMetricsServiceResponse>, Status> {
        self.store.ingest_metrics(request.into_inner());
        Ok(Response::new(ExportMetricsServiceResponse::default()))
    }
}

struct LogsEndpoint {
    store: Arc<TelemetryStore>,
}

#[tonic::async_trait]
impl LogsService for LogsEndpoint {
    async fn export(
        &self,
        request: Request<ExportLogsServiceRequest>,
    ) -> Result<Response<ExportLogsServiceResponse>, Status> {
        self.store.ingest_logs(request.into_inner());
        Ok(Response::new(ExportLogsServiceResponse::default()))
    }
}

struct TraceEndpoint {
    store: Arc<TelemetryStore>,
}

#[tonic::async_trait]
impl TraceService for TraceEndpoint {
    async fn export(
        &self,
        request: Request<ExportTraceServiceRequest>,
    ) -> Result<Response<ExportTraceServiceResponse>, Status> {
        self.store.ingest_traces(request.into_inner());
        Ok(Response::new(ExportTraceServiceResponse::default()))
    }
}
