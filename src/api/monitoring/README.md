# Vault Monitoring Stack

This directory contains the configuration for the Vault observability stack.

## Quick Start

### 1. Start Monitoring Services

```bash
# From src/api directory
docker-compose -f docker-compose.observability.yml up -d

# Verify all services are running
docker-compose -f docker-compose.observability.yml ps
```

### 2. Access Dashboards

- **Grafana**: http://localhost:3000 (admin/changeme)
- **Prometheus**: http://localhost:9090
- **Alertmanager**: http://localhost:9093

### 3. View Application Metrics

Once your Vault application is running:
- **Metrics**: http://localhost:8000/api/v1/monitoring/metrics
- **Health**: http://localhost:8000/api/v1/monitoring/health/ready

## Configuration Files

### Prometheus (`prometheus.yml`)
- Defines scrape targets (Vault API, PostgreSQL, Redis)
- Configures alert rules
- Sets retention period (30 days)

### Alertmanager (`alertmanager.yml`)
- Routes alerts to notification channels
- Defines receiver groups (email, Slack, PagerDuty)
- Configures alert grouping and throttling

### Alert Rules (`alerts.yml`)
- Application alerts (error rate, latency, downtime)
- Database alerts (connections, slow queries)
- System alerts (CPU, memory, disk)
- Business alerts (indexing failures, slow searches)

### Loki (`loki.yml`)
- Log aggregation configuration
- Retention policy (30 days)
- Storage configuration

### Promtail (`promtail.yml`)
- Log shipping from files to Loki
- Log parsing and label extraction

### Grafana Datasources (`grafana-datasources.yml`)
- Prometheus connection
- Loki connection
- PostgreSQL connection

### Grafana Dashboards (`grafana-dashboards/`)
- `application-dashboard.json` - HTTP metrics, errors, latency
- `business-metrics-dashboard.json` - Files, searches, exports
- `system-dashboard.json` - CPU, memory, disk, cache

## Environment Variables

Create a `.env` file in the monitoring directory:

```env
# Grafana
GRAFANA_ADMIN_USER=admin
GRAFANA_ADMIN_PASSWORD=changeme

# PostgreSQL Exporter
POSTGRES_EXPORTER_DSN=postgresql://vault:vault@postgres:5432/vault?sslmode=disable

# Redis Exporter
REDIS_ADDR=redis:6379

# Alertmanager
SLACK_WEBHOOK_URL=https://hooks.slack.com/services/YOUR/WEBHOOK/URL
SMTP_HOST=smtp.gmail.com:587
SMTP_FROM=alerts@vault.dev
SMTP_USERNAME=your-email@gmail.com
SMTP_PASSWORD=your-app-password

ALERT_EMAIL=ops@vault.dev
CRITICAL_EMAIL=ops@vault.dev
SECURITY_EMAIL=security@vault.dev
```

## Customization

### Adding Custom Metrics

1. Define metric in `src/api/src/monitoring/metrics.py`
2. Use metric in your code
3. Metrics automatically exposed at `/api/v1/monitoring/metrics`

### Creating Custom Dashboards

1. Create dashboard in Grafana UI
2. Export as JSON
3. Save to `grafana-dashboards/`
4. Restart Grafana to load

### Adding Custom Alerts

1. Edit `alerts.yml`
2. Add new alert rule
3. Reload Prometheus: `curl -X POST http://localhost:9090/-/reload`

### Configuring Notifications

Edit `alertmanager.yml`:

- **Email**: Update `smtp_*` settings and receiver email addresses
- **Slack**: Add webhook URL and channel names
- **PagerDuty**: Add service key to critical alerts receiver

## Architecture

```
┌─────────────────┐
│   Vault API     │
│  :8000/metrics  │
└────────┬────────┘
         │
         ├─────────────┐
         │             │
    ┌────▼────┐   ┌───▼────┐
    │Prometheus│   │  Loki  │
    │  :9090   │   │ :3100  │
    └────┬────┘   └───┬────┘
         │             │
    ┌────▼─────────────▼────┐
    │      Grafana          │
    │       :3000           │
    └───────────────────────┘
         │
    ┌────▼────────┐
    │Alertmanager │
    │    :9093    │
    └─────┬───────┘
          │
    ┌─────▼──────────────┐
    │  Notifications     │
    │ Email/Slack/etc    │
    └────────────────────┘
```

## Troubleshooting

### Prometheus Not Scraping Vault

1. Check Vault is exposing metrics: `curl http://localhost:8000/api/v1/monitoring/metrics`
2. Check Prometheus targets: http://localhost:9090/targets
3. Verify network connectivity between containers
4. Check Prometheus logs: `docker logs vault-prometheus`

### Grafana Dashboards Not Loading

1. Check dashboard files are in `grafana-dashboards/`
2. Restart Grafana: `docker restart vault-grafana`
3. Check Grafana logs: `docker logs vault-grafana`
4. Import manually via Grafana UI

### Alerts Not Firing

1. Check alert rules: http://localhost:9090/alerts
2. Verify thresholds are correct
3. Check Alertmanager status: http://localhost:9093/#/status
4. Test alert manually:
   ```bash
   curl -X POST http://localhost:9093/api/v1/alerts -d '[
     {
       "labels": {"alertname": "Test", "severity": "warning"},
       "annotations": {"summary": "Test alert"}
     }
   ]'
   ```

### Logs Not Appearing in Loki

1. Check Promtail is running: `docker ps | grep promtail`
2. Verify log file paths exist
3. Check Promtail logs: `docker logs vault-promtail`
4. Test Loki query: http://localhost:3000/explore (select Loki datasource)

## Maintenance

### Backup Monitoring Data

```bash
# Backup Prometheus data
docker run --rm -v prometheus-data:/data -v $(pwd):/backup ubuntu tar czf /backup/prometheus-backup.tar.gz /data

# Backup Grafana data
docker run --rm -v grafana-data:/data -v $(pwd):/backup ubuntu tar czf /backup/grafana-backup.tar.gz /data
```

### Clean Up Old Data

```bash
# Prometheus retention is configured in prometheus.yml (30 days)
# Loki retention is configured in loki.yml (30 days)

# Manual cleanup if needed
docker-compose -f docker-compose.observability.yml down -v
docker volume rm prometheus-data loki-data
```

### Update Configuration

```bash
# Reload Prometheus config (without restart)
curl -X POST http://localhost:9090/-/reload

# Reload Alertmanager config
curl -X POST http://localhost:9093/-/reload

# Restart other services
docker-compose -f docker-compose.observability.yml restart grafana loki
```

## Production Recommendations

1. **Security**:
   - Change default Grafana password
   - Enable authentication on Prometheus
   - Use TLS for all connections
   - Restrict access with firewall rules

2. **High Availability**:
   - Run multiple Prometheus instances
   - Use Thanos for long-term storage
   - Configure Alertmanager clustering
   - Use external PostgreSQL for Grafana

3. **Performance**:
   - Adjust scrape intervals based on needs
   - Limit metric cardinality
   - Use recording rules for complex queries
   - Configure appropriate retention periods

4. **Backup**:
   - Regular backups of Prometheus data
   - Backup Grafana dashboards (use provisioning)
   - Backup alert rules to git repository

5. **Monitoring the Monitors**:
   - Set up alerts for Prometheus down
   - Monitor Grafana availability
   - Check Loki disk usage

## References

- [Parent Documentation](../MONITORING.md)
- [Operations Runbook](../RUNBOOK.md)
- [Alert Definitions](../ALERTS.md)
- [Prometheus Docs](https://prometheus.io/docs/)
- [Grafana Docs](https://grafana.com/docs/)
- [Loki Docs](https://grafana.com/docs/loki/)
