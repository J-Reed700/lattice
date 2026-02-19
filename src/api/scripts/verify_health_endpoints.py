#!/usr/bin/env python3
"""
Verify health check endpoints are properly configured.

This script checks that all health endpoints are accessible
and return the expected response structure.
"""

import sys
import asyncio
from pathlib import Path

sys.path.insert(0, str(Path(__file__).parent.parent))

from src.api.app import create_app
from fastapi.testclient import TestClient


def test_health_endpoints():
    """Test all health check endpoints."""
    app = create_app()
    client = TestClient(app)

    print("Testing Health Check Endpoints")
    print("=" * 60)

    tests_passed = 0
    tests_failed = 0

    # Test 1: Basic health check
    print("\n1. Testing GET /api/v1/health/")
    try:
        response = client.get("/api/v1/health/")
        print(f"   Status: {response.status_code}")
        if response.status_code == 200:
            data = response.json()
            assert "status" in data, "Missing 'status' field"
            assert "version" in data, "Missing 'version' field"
            assert "timestamp" in data, "Missing 'timestamp' field"
            assert "uptime_seconds" in data, "Missing 'uptime_seconds' field"
            print(f"   Response: {data}")
            print("   ✓ PASSED")
            tests_passed += 1
        else:
            print(f"   ✗ FAILED: Expected 200, got {response.status_code}")
            tests_failed += 1
    except Exception as e:
        print(f"   ✗ FAILED: {e}")
        tests_failed += 1

    # Test 2: Liveness check
    print("\n2. Testing GET /api/v1/health/live")
    try:
        response = client.get("/api/v1/health/live")
        print(f"   Status: {response.status_code}")
        if response.status_code == 200:
            data = response.json()
            assert data["alive"] is True, "alive should be True"
            assert "timestamp" in data, "Missing 'timestamp' field"
            print(f"   Response: {data}")
            print("   ✓ PASSED")
            tests_passed += 1
        else:
            print(f"   ✗ FAILED: Expected 200, got {response.status_code}")
            tests_failed += 1
    except Exception as e:
        print(f"   ✗ FAILED: {e}")
        tests_failed += 1

    # Test 3: Readiness check
    print("\n3. Testing GET /api/v1/health/ready")
    try:
        response = client.get("/api/v1/health/ready")
        print(f"   Status: {response.status_code}")
        if response.status_code in [200, 503]:
            data = response.json()
            assert "ready" in data, "Missing 'ready' field"
            assert "services" in data, "Missing 'services' field"
            assert "timestamp" in data, "Missing 'timestamp' field"
            print(f"   Response: {data}")
            print("   ✓ PASSED")
            tests_passed += 1
        else:
            print(f"   ✗ FAILED: Expected 200 or 503, got {response.status_code}")
            tests_failed += 1
    except Exception as e:
        print(f"   ✗ FAILED: {e}")
        tests_failed += 1

    # Test 4: System status
    print("\n4. Testing GET /api/v1/health/status")
    try:
        response = client.get("/api/v1/health/status")
        print(f"   Status: {response.status_code}")
        if response.status_code in [200, 503]:
            data = response.json()
            assert "overall_status" in data, "Missing 'overall_status' field"
            assert "components" in data, "Missing 'components' field"
            assert "timestamp" in data, "Missing 'timestamp' field"
            print(f"   Response keys: {list(data.keys())}")
            print(f"   Components: {list(data['components'].keys())}")
            print("   ✓ PASSED")
            tests_passed += 1
        else:
            print(f"   ✗ FAILED: Expected 200 or 503, got {response.status_code}")
            tests_failed += 1
    except Exception as e:
        print(f"   ✗ FAILED: {e}")
        tests_failed += 1

    # Test 5: Metrics endpoint
    print("\n5. Testing GET /api/v1/health/metrics")
    try:
        response = client.get("/api/v1/health/metrics")
        print(f"   Status: {response.status_code}")
        if response.status_code == 200:
            data = response.json()
            assert "timestamp" in data, "Missing 'timestamp' field"
            assert "system" in data, "Missing 'system' field"
            assert "memory" in data, "Missing 'memory' field"
            assert "gc" in data, "Missing 'gc' field"
            print(f"   System: {data['system']['platform']}")
            print(f"   Memory RSS: {data['memory']['rss_mb']:.2f} MB")
            print(f"   CPU: {data['system']['cpu_percent']:.1f}%")
            print("   ✓ PASSED")
            tests_passed += 1
        else:
            print(f"   ✗ FAILED: Expected 200, got {response.status_code}")
            tests_failed += 1
    except Exception as e:
        print(f"   ✗ FAILED: {e}")
        tests_failed += 1

    # Test 6: Check OpenAPI docs include health endpoints
    print("\n6. Testing OpenAPI documentation")
    try:
        response = client.get("/api/openapi.json")
        if response.status_code == 200:
            openapi = response.json()
            paths = openapi.get("paths", {})
            health_paths = [p for p in paths.keys() if "/health" in p]
            print(f"   Health endpoints in OpenAPI: {len(health_paths)}")
            for path in health_paths:
                print(f"     - {path}")
            assert len(health_paths) >= 4, "Should have at least 4 health endpoints"
            print("   ✓ PASSED")
            tests_passed += 1
        else:
            print(f"   OpenAPI docs not available (status {response.status_code})")
            print("   ⚠ SKIPPED")
    except Exception as e:
        print(f"   ✗ FAILED: {e}")
        tests_failed += 1

    # Summary
    print("\n" + "=" * 60)
    print(f"Tests Passed: {tests_passed}")
    print(f"Tests Failed: {tests_failed}")
    print("=" * 60)

    if tests_failed == 0:
        print("\n✓ All health check endpoints are working correctly!")
        return 0
    else:
        print(f"\n✗ {tests_failed} test(s) failed")
        return 1


if __name__ == "__main__":
    sys.exit(test_health_endpoints())
