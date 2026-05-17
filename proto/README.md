# Vendored protobuf schemas

`opamp.proto` and `anyvalue.proto` are vendored verbatim from the OpAMP
specification so the build is reproducible offline.

| File | Source |
|------|--------|
| `opamp.proto` | <https://github.com/open-telemetry/opamp-spec/blob/main/proto/opamp.proto> |
| `anyvalue.proto` | <https://github.com/open-telemetry/opamp-spec/blob/main/proto/anyvalue.proto> |

Both declare package `opamp.proto.v1`. They are compiled to Rust by
`crates/otelc-opamp/build.rs` via `prost-build`.

To refresh, re-download both files from the `main` branch of `opamp-spec`
and record the commit here.

OTLP protobuf schemas are **not** vendored: the `otelc-otlp` crate uses the
`opentelemetry-proto` crate, which ships both the message types and the
generated tonic gRPC service stubs.
