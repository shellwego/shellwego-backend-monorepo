# ShellWeGo Observability
**Introspection.** Stop flying blind.

- **Metrics:** Native Prometheus exporter with pre-built Grafana dashboards.
- **Logs:** Loki-compatible structured log aggregation.
- **Tracing:** OpenTelemetry integration for distributed request tracking.
- **Profiles:** Hooks for continuous profiling (Flamegraphs).

## Quick Start

```rust
use shellwego_observability::{init, ObservabilityConfig};

let config = ObservabilityConfig::default()
    .with_service_name("my-service")
    .production();

let handle = init(&config).await?;
// Metrics available at http://0.0.0.0:9090/metrics
```

## Registered Metrics

### Built-in ShellWeGo Metrics

| Metric | Type | Labels | Description |
|--------|------|--------|-------------|
| `shellwego_microvm_spawn_duration_seconds` | Histogram | `node`, `runtime` | Time taken to spawn a microVM |
| `shellwego_node_memory_usage_bytes` | Gauge | `node`, `type` | Current memory usage (total/used/available) |
| `shellwego_node_memory_pressure` | Gauge | `node` | Memory pressure ratio (0.0–1.0) |
| `shellwego_apps_running` | Gauge | `node`, `status` | Number of running applications |
| `shellwego_network_bytes_total` | Counter | `node`, `direction`, `interface` | Total network bytes transferred |
| `shellwego_network_dropped_packets_total` | Counter | `node`, `interface`, `direction` | Total network packets dropped |
| `shellwego_storage_pool_usage` | Gauge | `node`, `pool` | Storage pool usage ratio (0.0–1.0) |
| `shellwego_deployment_count_total` | Counter | `node`, `status`, `runtime` | Total number of deployments |
| `shellwego_http_request_duration_seconds` | Histogram | `method`, `path`, `status` | HTTP request duration |
| `shellwego_active_connections` | Gauge | `node`, `type` | Number of active connections |

### Process Metrics

When enabled via `enable_process_metrics`, the following Prometheus process metrics are automatically collected:
- `process_resident_memory_bytes` — RSS memory
- `process_cpu_seconds_total` — CPU usage
- `process_start_time_seconds` — Start timestamp
- `process_open_fds` — Open file descriptors

## Helper Functions

```rust
use shellwego_observability::metrics::builtin;

// Update memory pressure (0.0 - 1.0)
builtin::set_memory_pressure("node-1", 0.75);

// Record dropped packets
builtin::inc_dropped_packets("node-1", "eth0", "ingress", 50);

// Update storage pool usage (0.0 - 1.0)
builtin::set_storage_usage("node-1", "default", 0.82);

// Record microVM spawn
builtin::MICROVM_SPAWN_DURATION
    .with_label_values(&["node-1", "firecracker"])
    .observe(1.5);
```

## Loki Push

Structured logs can be pushed to a Loki instance:

```rust
let config = ObservabilityConfig::default()
    .with_service_name("my-service")
    .with_loki_url("http://loki:3100");
```

## OTLP Export

Distributed tracing via OpenTelemetry OTLP:

```rust
let config = ObservabilityConfig::default()
    .with_service_name("my-service")
    .with_otlp_endpoint("http://otel-collector:4317");
```

## Grafana Dashboards

Five pre-built dashboards are available in `config/grafana/dashboards/`:

1. **Platform Overview** — Cluster-wide health, deployment stats, resource gauges
2. **Node Resources** — Per-node CPU, memory, disk, microVM density
3. **MicroVM Performance** — Spawn duration percentiles, deployment success rates
4. **Network Observability** — Throughput, dropped packets, active connections
5. **Control Plane Health** — HTTP latency, error rates, request distribution

Dashboards are auto-provisioned via `config/grafana/provisioning/`.

## Alerting

Prometheus alert rules are defined in `config/prometheus_alerts.yml`:

| Alert | Threshold | Severity |
|-------|-----------|----------|
| `MicroVMSpawnDurationHigh` | p95 > 5s | warning |
| `NodeMemoryPressureHigh` | > 0.8 | critical |
| `NetworkDroppedPacketsHigh` | > 100/min | warning |
| `StoragePoolUsageHigh` | > 0.85 | critical |
| `ControlPlaneDown` | unreachable > 1m | critical |
| `AgentDown` | unreachable > 2m | warning |

## Docker Compose

The full monitoring stack (Prometheus, Loki, Grafana, Alertmanager) is included in `docker-compose.yml`:

```bash
docker-compose up -d prometheus alertmanager loki grafana
```

- Grafana: http://localhost:3000 (admin/admin)
- Prometheus: http://localhost:9090
- Loki: http://localhost:3100
- Alertmanager: http://localhost:9093
