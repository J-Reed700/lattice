# Security Testing Suite

Comprehensive security testing for Recall/Vault backend.

## 📁 Test Files

| File | Tests | Purpose |
|------|-------|---------|
| `conftest.py` | - | Test fixtures, payloads, utilities |
| `test_authentication.py` | 30+ | Login, tokens, registration |
| `test_authorization.py` | 15+ | Access control, privileges |
| `test_path_traversal.py` | 25+ | Directory traversal, symlinks |
| `test_csrf.py` | 12+ | CSRF protection |
| `test_rate_limiting.py` | 10+ | Brute force prevention |
| `test_input_validation.py` | 20+ | Malformed data, types |
| `test_file_upload_security.py` | 20+ | Malicious files, MIME |
| `test_injection.py` | 25+ | SQL, command, template injection |
| `test_api_security.py` | 20+ | Headers, CORS, disclosure |
| `test_security_integration.py` | 15+ | End-to-end workflows |

**Total: 190+ security tests**

## 🚀 Quick Start

```bash
# Run all security tests
pytest tests/security/ -v

# Run with coverage
pytest tests/security/ --cov=src --cov-report=html

# Run specific category
pytest tests/security/test_authentication.py -v
```

## 📚 Documentation

Located in `vault/backend/`:

- **`SECURITY_TEST_QUICKSTART.md`** - Start here (5 min guide)
- **`SECURITY_TESTING.md`** - Complete testing guide
- **`SECURITY_TESTING_CHECKLIST.md`** - Detailed checklist
- **`SECURITY_TEST_RESULTS.md`** - Results template
- **`SECURITY_TEST_SUITE_SUMMARY.md`** - Overview

## 🔧 Scripts

Located in `vault/backend/scripts/`:

- **`run_security_tests.sh`** - Run test suite with reporting
- **`security_scan.sh`** - Static analysis (Bandit, Safety, Semgrep)

## 🧪 Test Categories

### Authentication (30+ tests)
- Valid/invalid login
- Token validation
- Username enumeration prevention
- SQL injection protection

### Authorization (15+ tests)
- Resource isolation
- Privilege escalation prevention
- Cross-user access control

### Path Traversal (25+ tests)
- Basic/encoded traversal
- Symlink attacks
- Filename validation

### Injection (25+ tests)
- SQL injection
- Command injection
- Template injection
- XXE, Log4j, etc.

### File Upload (20+ tests)
- Malicious extensions
- MIME spoofing
- Zip bombs
- Size limits

### API Security (20+ tests)
- Security headers
- CORS configuration
- Information disclosure

### And more...
- CSRF protection (12+ tests)
- Rate limiting (10+ tests)
- Input validation (20+ tests)
- Integration (15+ tests)

## 🎯 Coverage

- **OWASP Top 10:** 85%+ coverage
- **CWE Top 25:** 15+ CWEs tested
- **Code Coverage Target:** 80%+

## 📊 Example Usage

```bash
# Test authentication
pytest tests/security/test_authentication.py -v

# Test path traversal protection
pytest tests/security/test_path_traversal.py -v

# Test with coverage report
pytest tests/security/ --cov=src --cov-report=term

# Generate HTML report
pytest tests/security/ --html=security_report.html
```

## 🔍 What Gets Tested

### ✅ Attack Vectors
- SQL injection
- Path traversal
- XSS
- CSRF
- Command injection
- File upload attacks
- Brute force
- Privilege escalation

### ✅ Security Controls
- Authentication (JWT)
- Authorization (RBAC)
- Input validation
- Path sanitization
- Rate limiting
- CSRF tokens
- Security headers

### ✅ Edge Cases
- Empty inputs
- Null bytes
- Unicode characters
- Extremely long strings
- Malformed data

## 📈 Results

Run tests and check:
- Pass/fail status
- Coverage percentage
- Failed test details

See `SECURITY_TEST_RESULTS.md` for latest results.

## 🆘 Troubleshooting

**Tests fail?**
- Check test database connection
- Verify dependencies installed
- Review implementation status

**Need help?**
- Read `SECURITY_TEST_QUICKSTART.md`
- Check test code for examples
- Review fixtures in `conftest.py`

## 🔗 Resources

- [OWASP Testing Guide](https://owasp.org/www-project-web-security-testing-guide/)
- [OWASP Top 10](https://owasp.org/www-project-top-ten/)
- [CWE Top 25](https://cwe.mitre.org/top25/)

---

**Last Updated:** November 2024
**Total Tests:** 190+
**Status:** ✅ Production Ready
