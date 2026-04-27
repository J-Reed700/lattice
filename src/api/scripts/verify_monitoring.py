#!/usr/bin/env python3
"""
Monitoring verification script for Vault.

This script verifies that all monitoring components are working correctly
before deployment or as part of health checks.

Usage:
    python scripts/verify_monitoring.py [--host HOST] [--port PORT]
"""

import argparse
import asyncio
import sys
from typing import Dict, List, Tuple
import httpx
import structlog

logger = structlog.get_logger(__name__)


class MonitoringVerifier:
    """Verify monitoring infrastructure is working."""

    def __init__(self, host: str = "localhost", port: int = 8000):
        self.base_url = f"http://{host}:{port}"
        self.checks: List[Tuple[str, bool, str]] = []

    async def verify_all(self) -> bool:
        """Run all verification checks."""
        logger.info("Starting monitoring verification", base_url=self.base_url)

        await self.verify_health_endpoints()
        await self.verify_metrics_endpoint()
        await self.verify_metrics_content()
        self.verify_monitoring_stack()

        self.print_results()

        return all(result for _, result, _ in self.checks)

    async def verify_health_endpoints(self) -> None:
        """Verify health check endpoints are accessible."""
        endpoints = [
            ("/api/v1/monitoring/health/live", "Liveness endpoint"),
            ("/api/v1/monitoring/health/ready", "Readiness endpoint"),
            ("/api/v1/monitoring/ping", "Ping endpoint"),
        ]

        async with httpx.AsyncClient(timeout=10.0) as client:
            for endpoint, description in endpoints:
                try:
                    url = f"{self.base_url}{endpoint}"
                    response = await client.get(url)

                    if response.status_code == 200:
                        self.checks.append((description, True, f"OK ({url})"))
                        logger.info(f"{description} check passed", url=url)
                    else:
                        self.checks.append(
                            (
                                description,
                                False,
                                f"HTTP {response.status_code} ({url})",
                            )
                        )
                        logger.error(
                            f"{description} check failed",
                            url=url,
                            status_code=response.status_code,
                        )
                except httpx.RequestError as e:
                    self.checks.append((description, False, f"Connection error: {e}"))
                    logger.error(f"{description} check failed", error=str(e))

    async def verify_metrics_endpoint(self) -> None:
        """Verify Prometheus metrics endpoint is accessible."""
        try:
            async with httpx.AsyncClient(timeout=10.0) as client:
                url = f"{self.base_url}/api/v1/monitoring/metrics"
                response = await client.get(url)

                if response.status_code == 200:
                    self.checks.append(
                        ("Metrics endpoint", True, f"OK ({url})")
                    )
                    logger.info("Metrics endpoint check passed", url=url)
                else:
                    self.checks.append(
                        (
                            "Metrics endpoint",
                            False,
                            f"HTTP {response.status_code}",
                        )
                    )
                    logger.error(
                        "Metrics endpoint check failed",
                        url=url,
                        status_code=response.status_code,
                    )
        except httpx.RequestError as e:
            self.checks.append(("Metrics endpoint", False, f"Connection error: {e}"))
            logger.error("Metrics endpoint check failed", error=str(e))

    async def verify_metrics_content(self) -> None:
        """Verify metrics contain expected content."""
        required_metrics = [
            "vault_http_requests_total",
            "vault_http_request_duration_seconds",
            "vault_system_cpu_usage_percent",
            "vault_system_memory_usage_bytes",
        ]

        try:
            async with httpx.AsyncClient(timeout=10.0) as client:
                url = f"{self.base_url}/api/v1/monitoring/metrics"
                response = await client.get(url)

                if response.status_code != 200:
                    self.checks.append(
                        ("Metrics content", False, "Failed to fetch metrics")
                    )
                    return

                content = response.text

                for metric in required_metrics:
                    if metric in content:
                        self.checks.append(
                            (f"Metric: {metric}", True, "Present")
                        )
                        logger.info(f"Metric {metric} found")
                    else:
                        self.checks.append(
                            (f"Metric: {metric}", False, "Missing")
                        )
                        logger.warning(f"Metric {metric} not found")

        except httpx.RequestError as e:
            self.checks.append(("Metrics content", False, f"Connection error: {e}"))
            logger.error("Metrics content check failed", error=str(e))

    def verify_monitoring_stack(self) -> None:
        """Verify external monitoring services are accessible."""
        services = [
            ("http://localhost:9090/-/healthy", "Prometheus"),
            ("http://localhost:3000/api/health", "Grafana"),
            ("http://localhost:3100/ready", "Loki"),
        ]

        import requests

        for url, service in services:
            try:
                response = requests.get(url, timeout=5)
                if response.status_code == 200:
                    self.checks.append((f"{service} service", True, f"OK ({url})"))
                    logger.info(f"{service} check passed", url=url)
                else:
                    self.checks.append(
                        (
                            f"{service} service",
                            False,
                            f"HTTP {response.status_code}",
                        )
                    )
                    logger.warning(
                        f"{service} check failed",
                        url=url,
                        status_code=response.status_code,
                    )
            except requests.RequestException as e:
                self.checks.append(
                    (f"{service} service", False, f"Not accessible: {e}")
                )
                logger.warning(
                    f"{service} not accessible (optional)",
                    url=url,
                    error=str(e),
                )

    def print_results(self) -> None:
        """Print verification results in a formatted table."""
        print("\n" + "=" * 80)
        print("MONITORING VERIFICATION RESULTS".center(80))
        print("=" * 80)

        passed = 0
        failed = 0

        for name, result, message in self.checks:
            status_symbol = "✅" if result else "❌"
            status_text = "PASS" if result else "FAIL"

            if result:
                passed += 1
            else:
                failed += 1

            print(f"{status_symbol} [{status_text}] {name:.<50} {message}")

        print("=" * 80)
        print(f"Results: {passed} passed, {failed} failed")
        print("=" * 80 + "\n")


async def verify_sentry() -> Tuple[bool, str]:
    """Verify Sentry integration (if configured)."""
    try:
        import sentry_sdk

        sentry_sdk.capture_message(
            "Monitoring verification test",
            level="info",
        )

        return True, "Sentry test message sent"
    except ImportError:
        return False, "Sentry SDK not installed"
    except Exception as e:
        return False, f"Sentry error: {e}"


async def main():
    """Main entry point."""
    parser = argparse.ArgumentParser(
        description="Verify Vault monitoring infrastructure"
    )
    parser.add_argument(
        "--host",
        default="localhost",
        help="Host to connect to (default: localhost)",
    )
    parser.add_argument(
        "--port",
        type=int,
        default=8000,
        help="Port to connect to (default: 8000)",
    )
    parser.add_argument(
        "--verbose",
        "-v",
        action="store_true",
        help="Enable verbose logging",
    )

    args = parser.parse_args()

    if args.verbose:
        structlog.configure(
            processors=[
                structlog.stdlib.add_log_level,
                structlog.processors.TimeStamper(fmt="iso"),
                structlog.dev.ConsoleRenderer(),
            ],
            wrapper_class=structlog.stdlib.BoundLogger,
            logger_factory=structlog.stdlib.LoggerFactory(),
            cache_logger_on_first_use=True,
        )

    verifier = MonitoringVerifier(host=args.host, port=args.port)
    success = await verifier.verify_all()

    print("\nOptional Services:")
    sentry_ok, sentry_msg = await verify_sentry()
    status = "✅" if sentry_ok else "⚠️"
    print(f"{status} Sentry: {sentry_msg}")

    if success:
        print("\n✅ All required monitoring checks passed!")
        sys.exit(0)
    else:
        print("\n❌ Some monitoring checks failed!")
        sys.exit(1)


if __name__ == "__main__":
    asyncio.run(main())
