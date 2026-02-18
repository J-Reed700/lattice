import logging
import os

from opentelemetry import metrics, trace
from opentelemetry.exporter.otlp.proto.grpc.metric_exporter import OTLPMetricExporter
from opentelemetry.exporter.otlp.proto.grpc.trace_exporter import OTLPSpanExporter
from opentelemetry.instrumentation.fastapi import FastAPIInstrumentor
from opentelemetry.instrumentation.psycopg2 import Psycopg2Instrumentor
from opentelemetry.instrumentation.requests import RequestsInstrumentor
from opentelemetry.instrumentation.sqlalchemy import SQLAlchemyInstrumentor
from opentelemetry.sdk.metrics import MeterProvider
from opentelemetry.sdk.metrics.export import PeriodicExportingMetricReader
from opentelemetry.sdk.resources import SERVICE_NAME, SERVICE_VERSION, Resource
from opentelemetry.sdk.trace import TracerProvider
from opentelemetry.sdk.trace.export import BatchSpanProcessor

logger = logging.getLogger(__name__)


def setup_otel(app) -> tuple[trace.Tracer, metrics.Meter]:
    enabled = os.getenv("OTEL_ENABLED", "false").lower() == "true"

    if not enabled:
        logger.info("OpenTelemetry disabled")
        return trace.get_tracer(__name__), metrics.get_meter(__name__)

    service_name = os.getenv("OTEL_SERVICE_NAME", "vault-backend")
    service_version = os.getenv("OTEL_SERVICE_VERSION", "1.0.0")
    otlp_endpoint = os.getenv("OTEL_EXPORTER_OTLP_ENDPOINT", "http://localhost:4317")

    resource = Resource(
        attributes={
            SERVICE_NAME: service_name,
            SERVICE_VERSION: service_version,
            "deployment.environment": os.getenv("ENVIRONMENT", "development"),
        }
    )

    tracer_provider = TracerProvider(resource=resource)
    otlp_trace_exporter = OTLPSpanExporter(endpoint=otlp_endpoint)
    tracer_provider.add_span_processor(BatchSpanProcessor(otlp_trace_exporter))
    trace.set_tracer_provider(tracer_provider)

    otlp_metric_exporter = OTLPMetricExporter(endpoint=otlp_endpoint)
    metric_reader = PeriodicExportingMetricReader(
        otlp_metric_exporter, export_interval_millis=30000
    )
    meter_provider = MeterProvider(resource=resource, metric_readers=[metric_reader])
    metrics.set_meter_provider(meter_provider)

    FastAPIInstrumentor.instrument_app(app)

    try:
        from src.db.connection import engine

        SQLAlchemyInstrumentor().instrument(engine=engine.sync_engine)
    except Exception as e:
        logger.warning(f"Could not instrument SQLAlchemy: {e}")

    Psycopg2Instrumentor().instrument()

    RequestsInstrumentor().instrument()

    logger.info(f"OpenTelemetry initialized for {service_name} v{service_version}")
    logger.info(f"Exporting to: {otlp_endpoint}")

    return trace.get_tracer(__name__), metrics.get_meter(__name__)


def get_tracer(name: str = __name__) -> trace.Tracer:
    return trace.get_tracer(name)


def get_meter(name: str = __name__) -> metrics.Meter:
    return metrics.get_meter(name)
