use opentelemetry::metrics::{Counter, Histogram};
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
    requests_total: Counter<u64>,
    latency_proxy_overhead: Histogram<f64>,
    latency_verification: Histogram<f64>,
    violations_total: Counter<u64>,
    false_abstentions: Counter<u64>,
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
        .with_service_name("astragraph-proxy")
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
    let tracer = tracer_provider.tracer("astragraph-proxy");
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

    let meter = global::meter("astragraph-proxy");

    let tracing_layer = tracing_opentelemetry::layer().with_tracer(tracer);
    let _ = tracing_subscriber::registry()
        .with(tracing_layer)
        .with(tracing_subscriber::fmt::layer())
        .try_init();

    let telemetry = Telemetry {
        requests_total: meter
            .u64_counter("astragraph.requests.total")
            .with_description("Total intercepted agent actions by status")
            .build(),
        latency_proxy_overhead: meter
            .f64_histogram("astragraph.latency.proxy_overhead")
            .with_description("Time added by AstraGraph to each request")
            .build(),
        latency_verification: meter
            .f64_histogram("astragraph.latency.verification")
            .with_description("Verifier scoring latency")
            .build(),
        violations_total: meter
            .u64_counter("astragraph.violations.total")
            .with_description("Total blocked policy violations")
            .build(),
        false_abstentions: meter
            .u64_counter("astragraph.verifier.false_abstentions")
            .with_description("Verifier abstentions resolved by fallback")
            .build(),
    };

    let _ = TELEMETRY.set(telemetry);
    let _ = INIT.set(());
}

pub fn record_request(status: &str, latency_ms: f64) {
    if let Some(telemetry) = TELEMETRY.get() {
        telemetry.requests_total.add(
            1,
            &[
                KeyValue::new("status", status.to_string()),
                KeyValue::new("service", "proxy"),
            ],
        );
        telemetry
            .latency_proxy_overhead
            .record(latency_ms, &[KeyValue::new("service", "proxy")]);
    }
}

pub fn record_verification_latency(latency_ms: f64) {
    if let Some(telemetry) = TELEMETRY.get() {
        telemetry
            .latency_verification
            .record(latency_ms, &[KeyValue::new("service", "proxy")]);
    }
}

pub fn record_violation(rule_id: &str) {
    if let Some(telemetry) = TELEMETRY.get() {
        telemetry.violations_total.add(
            1,
            &[
                KeyValue::new("service", "proxy"),
                KeyValue::new("rule_id", rule_id.to_string()),
            ],
        );
    }
}

pub fn record_false_abstention() {
    if let Some(telemetry) = TELEMETRY.get() {
        telemetry
            .false_abstentions
            .add(1, &[KeyValue::new("service", "proxy")]);
    }
}
