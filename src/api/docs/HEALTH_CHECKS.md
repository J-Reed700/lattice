# Health Check Endpoints

Comprehensive health check and monitoring endpoints for the Vault API backend.

## Overview

The Vault API provides multiple health check endpoints designed for different use cases:

- **Liveness Probe** (`/health/live`) - Is the service alive?
- **Readiness Probe** (`/health/ready`) - Is the service ready to handle traffic?
- **Overall Health** (`/health/`) - General health status with component checks
- **Detailed Status** (`/health/status`) - Comprehensive component status
- **Metrics** (`/health/metrics`) - System performance metrics

## Endpoints

### GET /api/v1/health/live

**Purpose:** Kubernetes liveness probe
**Use Case:** Determine if the container should be restarted

```bash
curl http://localhost:8000/api/v1/health/live
```

**Response:**
```json
{
  "alive": true,
  "timestamp": "2025-11-11T10:30:00.123Z"
}
```

**Status Codes:**
- `200 OK` - Service is alive

This endpoint always returns 200 unless the application is completely hung.

---

### GET /api/v1/health/ready

**Purpose:** Kubernetes readiness probe
**Use Case:** Determine if the service should receive traffic

```bash
curl http://localhost:8000/api/v1/health/ready
```

**Response:**
```json
{
  "ready": true,
  "timestamp": "2025-11-11T10:30:00.123Z",
  "services": {
    "database": "ready",
    "vector_store": "ready",
    "embeddings": "ready"
  }
}
```

**Status Codes:**
- `200 OK` - Service is ready or not ready (check `ready` field)

**Service States:**
- `"ready"` - Service is operational
- `"not ready: <reason>"` - Service is not available
- `"error: <message>"` - Service has an error
- `"slow (<time>ms)"` - Service is responding but slow

---

### GET /api/v1/health/

**Purpose:** Load balancer health check
**Use Case:** Monitor overall application health

```bash
curl http://localhost:8000/api/v1/health/
```

**Response:**
```json
{
  "status": "healthy",
  "version": "1.0.0",
  "timestamp": "2025-11-11T10:30:00.123Z",
  "uptime_seconds": 12345.67
}
```

**Status Values:**
- `"healthy"` - All components operational
- `"degraded"` - Some components have issues
- `"unhealthy"` - Critical components are down

**Status Codes:**
- `200 OK` - Always returns 200 (check `status` field for details)

---

### GET /api/v1/health/status

**Purpose:** Detailed system monitoring
**Use Case:** Debug issues and monitor component health

```bash
curl http://localhost:8000/api/v1/health/status
```

**Response:**
```json
{
  "overall_status": "healthy",
  "timestamp": "2025-11-11T10:30:00.123Z",
  "components": {
    "database": {
      "status": "healthy",
      "latency_ms": 12.5,
      "details": {
        "connection": "active"
      }
    },
    "vector_store": {
      "status": "healthy",
      "details": {
        "pgvector_enabled": true
      }
    },
    "embeddings": {
      "status": "healthy",
      "details": {
        "model": "available"
      }
    },
    "file_watcher": {
      "status": "healthy",
      "details": {
        "enabled": true
      }
    }
  }
}
```

**Component Status:**
- `"healthy"` - Component is operational
- `"degraded"` - Component has performance issues
- `"unhealthy"` - Component is not operational

---

### GET /api/v1/health/metrics

**Purpose:** System metrics for monitoring
**Use Case:** Performance monitoring and alerting

```bash
curl http://localhost:8000/api/v1/health/metrics
```

**Response:**
```json
{
  "timestamp": "2025-11-11T10:30:00.123Z",
  "system": {
    "platform": "Linux-5.15.0-x86_64",
    "python_version": "3.11.5",
    "cpu_count": 8,
    "cpu_percent": 15.3
  },
  "memory": {
    "rss_mb": 1024.5,
    "vms_mb": 2048.7,
    "percent": 12.5
  },
  "gc": {
    "collections": {
      "gen0": 45,
      "gen1": 12,
      "gen2": 3
    }
  }
}
```

---

## Kubernetes Integration

### Deployment Configuration

```yaml
apiVersion: apps/v1
kind: Deployment
metadata:
  name: vault-backend
spec:
  template:
    spec:
      containers:
      - name: backend
        image: vault-backend:latest
        ports:
        - containerPort: 8000
        livenessProbe:
          httpGet:
            path: /api/v1/health/live
            port: 8000
          initialDelaySeconds: 30
          periodSeconds: 10
          timeoutSeconds: 5
          failureThreshold: 3
        readinessProbe:
          httpGet:
            path: /api/v1/health/ready
            port: 8000
          initialDelaySeconds: 10
          periodSeconds: 5
          timeoutSeconds: 3
          failureThreshold: 3
        startupProbe:
          httpGet:
            path: /api/v1/health/live
            port: 8000
          initialDelaySeconds: 0
          periodSeconds: 5
          failureThreshold: 30
```

### Probe Configuration Guidelines

**Liveness Probe:**
- `initialDelaySeconds: 30` - Wait for app to start
- `periodSeconds: 10` - Check every 10 seconds
- `failureThreshold: 3` - Restart after 3 failures (30 seconds)

**Readiness Probe:**
- `initialDelaySeconds: 10` - Start checking early
- `periodSeconds: 5` - Check frequently
- `failureThreshold: 3` - Mark unready after 3 failures (15 seconds)

**Startup Probe:**
- Use for apps with slow startup (ML model loading)
- `failureThreshold: 30` with `periodSeconds: 5` = 150 second startup window

---

## Docker Compose Integration

```yaml
services:
  backend:
    image: vault-backend:latest
    ports:
      - "8000:8000"
    healthcheck:
      test: ["CMD", "curl", "-f", "http://localhost:8000/api/v1/health/live"]
      interval: 30s
      timeout: 10s
      retries: 3
      start_period: 40s
    depends_on:
      postgres:
        condition: service_healthy
      redis:
        condition: service_healthy
```

---

## Load Balancer Integration

### AWS Application Load Balancer

```
Health Check Path: /api/v1/health/ready
Healthy Threshold: 2
Unhealthy Threshold: 3
Timeout: 5 seconds
Interval: 30 seconds
Success Codes: 200
```

### NGINX Upstream Health Checks

```nginx
upstream vault_backend {
    server backend1:8000;
    server backend2:8000;
    server backend3:8000;

    check interval=3000 rise=2 fall=3 timeout=1000 type=http;
    check_http_send "GET /api/v1/health/live HTTP/1.0\r\n\r\n";
    check_http_expect_alive http_2xx;
}
```

---

## Monitoring Integration

### Prometheus Scraping

While `/health/metrics` provides basic metrics, for production use Prometheus metrics:

```python
# Future enhancement: Add Prometheus endpoint
from prometheus_client import generate_latest

@router.get("/prometheus")
async def prometheus_metrics():
    return Response(
        content=generate_latest(),
        media_type="text/plain"
    )
```

### Datadog Integration

```python
# Example statsd metrics
from datadog import statsd

# In health check
statsd.gauge('vault.health.database.latency', latency_ms)
statsd.gauge('vault.health.memory.rss_mb', memory_rss)
```

---

## Alerting Rules

### Critical Alerts

**Service Down**
```
Alert when: ready = false for > 5 minutes
Action: Page on-call engineer
```

**Database Slow**
```
Alert when: database.latency_ms > 1000 for > 2 minutes
Action: Notify database team
```

### Warning Alerts

**Service Degraded**
```
Alert when: status = "degraded" for > 10 minutes
Action: Create ticket
```

**High Memory Usage**
```
Alert when: memory.percent > 80 for > 5 minutes
Action: Notify team
```

---

## Testing Health Checks

### Manual Testing

```bash
# Test all endpoints
curl http://localhost:8000/api/v1/health/
curl http://localhost:8000/api/v1/health/live
curl http://localhost:8000/api/v1/health/ready
curl http://localhost:8000/api/v1/health/status
curl http://localhost:8000/api/v1/health/metrics

# Pretty print JSON
curl http://localhost:8000/api/v1/health/status | jq

# Watch health status
watch -n 5 'curl -s http://localhost:8000/api/v1/health/status | jq'
```

### Automated Testing

```bash
# Run health check tests
cd src/api
pytest tests/unit/api/test_health.py -v

# Run with coverage
pytest tests/unit/api/test_health.py --cov=src.api.v1.health
```

---

## Troubleshooting

### Service Not Ready

If `/health/ready` returns `ready: false`:

1. Check individual service status in response
2. Review logs for error messages
3. Verify database connectivity: `psql -h localhost -U vault`
4. Check if pgvector extension is installed
5. Verify embedding models are downloaded

### High Latency

If database latency > 1000ms:

1. Check database connection pool settings
2. Verify database server performance
3. Check for long-running queries
4. Review network connectivity

### Memory Issues

If memory usage is high:

1. Check `/health/metrics` for memory breakdown
2. Review garbage collection statistics
3. Check for memory leaks in application code
4. Consider increasing container memory limits

---

## Best Practices

### Development

1. Always test health endpoints locally before deploying
2. Use health checks to gate deployment in CI/CD
3. Monitor health endpoints during load testing

### Production

1. Configure appropriate timeouts for your workload
2. Set up alerting on health check failures
3. Use readiness probes to prevent traffic to unhealthy instances
4. Monitor health check metrics in dashboards
5. Review health check logs regularly

### Security

1. Health endpoints are public by design (no auth required)
2. Don't expose sensitive information in health responses
3. Rate limit health endpoints if needed
4. Consider separate internal/external health endpoints

---

## Future Enhancements

- [ ] Add Prometheus metrics endpoint (`/metrics` format)
- [ ] Add queue depth metrics (Celery tasks)
- [ ] Add storage usage metrics (MinIO)
- [ ] Add cache hit rate metrics (Redis)
- [ ] Add custom business metrics
- [ ] Add distributed tracing integration
- [ ] Add detailed error responses with troubleshooting hints
