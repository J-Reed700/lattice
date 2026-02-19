"""Test script to verify all endpoints require authentication."""

import requests
import sys
from typing import List, Tuple

BASE_URL = "http://localhost:8000"

# All endpoints that should require auth
PROTECTED_ENDPOINTS = [
    ("POST", "/api/v1/files/"),
    ("GET", "/api/v1/files/"),
    ("GET", "/api/v1/files/test-id"),
    ("DELETE", "/api/v1/files/test-id"),
    ("POST", "/api/v1/documents/"),
    ("GET", "/api/v1/documents/list"),
    ("POST", "/api/v1/export/full"),
    ("GET", "/api/v1/search/"),
    ("GET", "/api/v1/tags/suggestions"),
    ("POST", "/api/v1/clusters/create"),
    ("POST", "/api/v1/summarize/text"),
    ("POST", "/api/v1/llm/ask"),
    ("GET", "/api/v1/watch/"),
    ("GET", "/api/v1/storage/stats"),
]

def test_endpoint_requires_auth(method: str, path: str) -> Tuple[bool, str]:
    """Test if endpoint returns 401 without auth."""
    try:
        url = f"{BASE_URL}{path}"
        if method == "GET":
            response = requests.get(url, timeout=5)
        elif method == "POST":
            response = requests.post(url, json={}, timeout=5)
        elif method == "DELETE":
            response = requests.delete(url, timeout=5)
        else:
            return False, f"Unsupported method: {method}"
        
        if response.status_code == 401:
            return True, "PASS - Returns 401 Unauthorized"
        else:
            return False, f"FAIL - Returns {response.status_code} (expected 401)"
    except requests.exceptions.ConnectionError:
        return False, "ERROR - Server not running"
    except Exception as e:
        return False, f"ERROR - {str(e)}"

def main():
    print("="*70)
    print("Authentication Test Suite")
    print("="*70)
    print(f"Testing {len(PROTECTED_ENDPOINTS)} endpoints...\n")
    
    passed = 0
    failed = 0
    errors = 0
    
    for method, path in PROTECTED_ENDPOINTS:
        success, message = test_endpoint_requires_auth(method, path)
        status = "PASS" if success else "FAIL"
        print(f"{status:6} {method:6} {path:40} {message}")
        
        if success:
            passed += 1
        elif "ERROR" in message:
            errors += 1
        else:
            failed += 1
    
    print("\n" + "="*70)
    print(f"Results: {passed} passed, {failed} failed, {errors} errors")
    print("="*70)
    
    if failed > 0:
        print("\nWARNING: Some endpoints are not properly protected!")
        sys.exit(1)
    elif errors > 0:
        print("\nERROR: Could not test all endpoints (server may not be running)")
        sys.exit(2)
    else:
        print("\nSUCCESS: All endpoints require authentication!")
        sys.exit(0)

if __name__ == "__main__":
    main()
