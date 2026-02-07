use opentelemetry::metrics::Gauge;
use opentelemetry::trace::TracerProvider;
use opentelemetry::{global, KeyValue};
use opentelemetry_otlp::{MetricExporter, SpanExporter, WithExportConfig};
use opentelemetry_sdk::metrics::SdkMeterProvider;
use opentelemetry_sdk::Resource;
use std::env;
use std::sync::OnceLock;
use tracing_subscriber::prelude::*;

#[allow(dead_code)]
#[derive(Clone)]
pub struct Telemetry {
    graph_nodes_total: Gauge<u64>,
}

static TELEMETRY: OnceLock<Telemetry> = OnceLock::new();
static INIT: OnceLock<()> = OnceLock::new();

pub fn init() {
    if INIT.get().is_some() {
        return;
    }
    let otlp_endpoint = env::var("OTEL_EXPORTER_OTLP_ENDPOINT")
        .unwrap_or_else(|_| "http://127.0.0.1:4317".to_string());
    let resource = Resource::builder()
        .with_service_name("astragraph-graph")
        .build();

    let span_exporter = SpanExporter::builder()
        .with_tonic()
        .with_endpoint(otlp_endpoint.clone())
        .build()
        .expect("span exporter");
    let tracer_provider = opentelemetry_sdk::trace::SdkTracerProvider::builder()
        .with_batch_exporter(span_exporter)
        .with_resource(resource.clone())
        .build();
    let tracer = tracer_provider.tracer("astragraph-graph");
    global::set_tracer_provider(tracer_provider);

    let metric_exporter = MetricExporter::builder()
        .with_tonic()
        .with_endpoint(otlp_endpoint)
        .build()
        .expect("metric exporter");
    let meter_provider = SdkMeterProvider::builder()
        .with_periodic_exporter(metric_exporter)
        .with_resource(resource)
        .build();
    global::set_meter_provider(meter_provider);
    let meter = global::meter("astragraph-graph");

    let tracing_layer = tracing_opentelemetry::layer().with_tracer(tracer);
    let _ = tracing_subscriber::registry()
        .with(tracing_layer)
        .with(tracing_subscriber::fmt::layer())
        .try_init();

    let telemetry = Telemetry {
        graph_nodes_total: meter
            .u64_gauge("astragraph.graph.nodes.total")
            .with_description("Current number of nodes in the active CCG")
            .build(),
    };

    let _ = TELEMETRY.set(telemetry);
    let _ = INIT.set(());
}

#[allow(dead_code)]
pub fn record_node_count(count: u64) {
    if let Some(telemetry) = TELEMETRY.get() {
        telemetry
            .graph_nodes_total
            .record(count, &[KeyValue::new("service", "graph")]);
    }
}
