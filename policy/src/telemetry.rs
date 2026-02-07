use opentelemetry::metrics::Counter;
use opentelemetry::trace::TracerProvider;
use opentelemetry::{global, KeyValue};
use opentelemetry_otlp::{MetricExporter, SpanExporter, WithExportConfig};
use opentelemetry_sdk::metrics::SdkMeterProvider;
use opentelemetry_sdk::Resource;
use std::env;
use std::sync::OnceLock;
use tracing_subscriber::prelude::*;

#[derive(Clone)]
pub struct Telemetry {
    evaluations_total: Counter<u64>,
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
        .with_service_name("astragraph-policy")
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
    let tracer = tracer_provider.tracer("astragraph-policy");
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
    let meter = global::meter("astragraph-policy");

    let tracing_layer = tracing_opentelemetry::layer().with_tracer(tracer);
    let _ = tracing_subscriber::registry()
        .with(tracing_layer)
        .with(tracing_subscriber::fmt::layer())
        .try_init();

    let telemetry = Telemetry {
        evaluations_total: meter
            .u64_counter("astragraph.policy.evaluations.total")
            .with_description("Total policy evaluations")
            .build(),
    };

    let _ = TELEMETRY.set(telemetry);
    let _ = INIT.set(());
}

pub fn record_evaluation(decision: &str) {
    if let Some(telemetry) = TELEMETRY.get() {
        telemetry.evaluations_total.add(
            1,
            &[
                KeyValue::new("decision", decision.to_string()),
                KeyValue::new("service", "policy"),
            ],
        );
    }
}
