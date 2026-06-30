# Elan — Federated Query System

Proof-of-concept for federated querying across multiple organisations' data stores using Apache DataFusion and Ballista.

## Architecture Overview

```
                          ┌──────────────────────────────────┐
                          │         elan-central             │
                          │  • SQLite catalog (datasets, IAM)│
                          │  • gRPC: CatalogSvc, IamSvc,     │
                          │    CoordinatorSvc, AuditSvc       │
                          └──────┬───────────┬───────────────┘
                                 │           │
                ┌────────────────┘           └─────────────────┐
                │                                              │
      ┌─────────▼──────────┐                      ┌───────────▼──────────┐
      │   elan-coordinator │  ←─── remote env ──→ │   elan-executor      │
      │  • Reads YAML/TOML │                      │  • Ballista server   │
      │  • Registers data- │                      │  • Registers local   │
      │    sets to central │                      │    datasets into DF  │
      │  • Auth check API  │◄─────────────────────│  • Checks auth via   │
      └────────────────────┘                      │    coordinator       │
                                                  └──────────────────────┘
                                                           ▲
                                                           │ Arrow Flight
                                                           │ (Ballista)
      ┌────────────────────────────┐              ┌────────┴─────────────┐
      │       elan-tui             │◄─── HTTP ───►│     elan-query       │
      │  • SQL editor (ratatui)    │              │  • Custom catalog    │
      │  • Results table view      │              │    provider (gRPC)   │
      │  • Audit event stream      │              │  • IAM enforcement   │
      │  • Catalog browser         │              │  • Ballista dispatch │
      └────────────────────────────┘              │  • Kafka audit       │
                                                  └──────────────────────┘
                                                           │
                                                  ┌────────▼──────────────┐
                                                  │     Kafka Cluster     │
                                                  │  • Audit event topics │
                                                  └───────────────────────┘
```

## Services

| Crate | Port | Role |
|---|---|---|
| `elan-central` | 50051 (gRPC) / 8080 (HTTP health) | Central catalog & IAM authority |
| `elan-coordinator` | 8081 (HTTP) | Remote environment coordinator |
| `elan-executor` | 50055 (Ballista) | Remote query executor |
| `elan-query` | 3001→3000 (HTTP) | Main DataFusion query service |
| `elan-tui` | — | Terminal UI |

## Crate Dependency Graph

```
elan-common  ←── elan-iam  ←── elan-central
     ↑                ↑         elan-executor
     │          elan-audit  ←── elan-query
     │
     └──── elan-coordinator
           elan-tui
```

## Critical Version Constraints

**Never change datafusion or ballista independently.** They must stay in sync:
- `datafusion = "53"` (53.1.0) + `datafusion-physical-plan = "53"` (needed for `Boundedness` / `EmissionType`)
- `ballista = "53.0.0"` + `ballista-executor = "53"` + `ballista-scheduler = "53"`
- `ballista-core = "53.0.0"`
- `arrow-* = "58"` (derived from datafusion 53 transitive dep)
- `tonic = "0.12"` for workspace gRPC services; `elan-executor` overrides to `tonic = "0.14"` to match `ballista-executor`/`ballista-core` internals (two tonic versions in dep graph is expected)
- `tui-textarea = "0.7"` (features = ["ratatui"]) to match `ratatui = "0.29"`

When upgrading: check that `ballista` has caught up to the new datafusion version first.

## Workspace Layout

```
elan/
├── Cargo.toml              # workspace; all version pinning lives here
├── AGENTS.md               # this file
├── proto/                  # Protobuf definitions for all gRPC services
│   ├── catalog.proto       # CatalogService (dataset lookup)
│   ├── coordinator.proto   # CoordinatorService (registration + heartbeat)
│   ├── iam.proto           # IamService (policy check + management)
│   └── audit.proto         # AuditService (streaming audit events)
├── migrations/             # SQLite schema migrations (sqlx)
│   ├── 0001_coordinators.sql
│   ├── 0002_datasets.sql
│   ├── 0003_iam.sql
│   └── 0004_audit_local.sql
├── config/
│   └── coordinator.example.toml
├── docker/
│   └── docker-compose.yml  # Kafka + Zookeeper
└── crates/
    ├── elan-common/        # Shared types, errors, proto re-exports
    ├── elan-iam/           # IAM/RBAC engine + DataFusion optimizer rule
    ├── elan-audit/         # Kafka audit event sink
    ├── elan-central/       # Central catalog + IAM gRPC service
    ├── elan-coordinator/   # Remote coordinator service
    ├── elan-executor/      # Ballista executor wrapper
    ├── elan-query/         # Main DataFusion query service
    └── elan-tui/           # Terminal UI
```

## Data Catalog (SQLite — elan-central)

Tables: `coordinators`, `datasets`, `iam_subjects`, `iam_group_members`, `iam_policies`, `audit_events`

See `migrations/` for full schema.

## IAM/RBAC Design

Enforcement happens in two layers inside `elan-query`:

1. **Catalog filter** (`elan-iam/src/catalog_filter.rs`): `SchemaProvider::table()` returns `None` for datasets the user cannot see — hides table existence.
2. **Physical optimizer rule** (`elan-iam/src/optimizer.rs`): After logical planning, the `IamFilterRule` physical optimizer traverses the plan tree:
   - Finds `RemoteTableScanExec` nodes (our custom execution node)
   - Calls `IamEngine::check(subject, resource, "SELECT")`
   - Deny → replaces node with `EmptyExec` (preserves schema, returns zero rows)
   - Allow with row filter → prepends `FilterExec` over the scan
   - Allow clean → passes through

Policy evaluation order: explicit Deny takes precedence over Allow; highest priority wins among same effect; groups are unioned.

## Ballista Integration

`elan-executor` starts a Ballista standalone server using `ballista-scheduler` + `ballista-executor`:
1. `ballista_scheduler::standalone::new_standalone_scheduler()` → returns a `SocketAddr`
2. Connects a `SchedulerGrpcClient` to that address via `tonic` 0.14
3. `ballista_executor::new_standalone_executor(scheduler_client, 1, BallistaCodec::default())` runs the executor loop

Datasets from the coordinator config are pre-registered into the executor's local DataFusion `SessionContext` at startup.

`elan-query`'s `RemoteTableScanExec::execute()` connects to the remote Ballista scheduler via `SessionContext::remote("df://host:port")` (the `ballista::prelude::SessionContextExt` extension trait), submits a pushed-down SQL fragment, and streams back record batches.

**Note:** `elan-executor` uses `tonic = "0.14"` directly (not workspace) to match ballista-core internals. This results in two tonic versions in the workspace — this is expected and intentional.

## Kafka Audit Events

Topics (one per event class):
- `elan.audit.query.submitted`
- `elan.audit.query.completed`
- `elan.audit.query.failed`
- `elan.audit.access.denied`
- `elan.audit.coordinator.registered`
- `elan.audit.dataset.registered`

Key = `query_id` (UUID v7). Format = JSON envelope with typed `payload` field.
`elan-central` also persists events to the `audit_events` SQLite table for the TUI's `AuditService.StreamAuditEvents` gRPC stream.

## API Surface

### elan-query (HTTP)

```
POST /api/v1/query    { sql, session_id }  → { query_id, columns, rows, duration_ms }
GET  /api/v1/catalog                       → { namespaces: [{ name, datasets: [...] }] }
GET  /health                               → { status: "ok" }
```

Auth: `Authorization: Bearer <username>` (username-as-token for PoC simplicity).

### elan-central (gRPC on :50051)

- `CatalogService` — GetDataset, ListDatasets, SearchDatasets
- `CoordinatorService` — Register, Heartbeat (bidirectional stream), RegisterDataset, UnregisterDataset
- `IamService` — CheckAccess, ListPolicies, CreatePolicy, DeletePolicy
- `AuditService` — StreamAuditEvents (server stream to TUI)

### elan-coordinator (HTTP on :8081)

```
GET  /health
GET  /auth/check?dataset=<name>&caller=<user>  → { allowed: bool, reason: str }
```

## TUI Keybindings

| Key | Action |
|---|---|
| `Tab` | Switch pane |
| `F5` / `Ctrl+Enter` | Execute SQL query |
| `Ctrl+C` | Quit |
| `↑/↓` | Scroll results / audit log |
| `Ctrl+L` | Clear SQL editor |
| `Ctrl+R` | Refresh catalog |

## Implementation Status

- [x] Workspace scaffold + Cargo.toml
- [x] Proto definitions (catalog, coordinator, iam, audit)
- [x] SQLite migrations
- [x] `elan-common`: types, errors, proto codegen
- [x] `elan-iam`: policy engine, catalog filter, optimizer rule
- [x] `elan-audit`: Kafka sink, event types
- [x] `elan-central`: catalog + IAM gRPC service
- [x] `elan-coordinator`: dataset registration + heartbeat
- [x] `elan-executor`: Ballista server + dataset registration
- [x] `elan-query`: DataFusion + custom catalog + IAM + HTTP API
- [x] `elan-tui`: ratatui SQL editor + results + audit stream

## Running the PoC

### Docker (all services)

```bash
docker compose up --build
# elan-central  :50051 (gRPC) / :8080 (HTTP)
# elan-query    :3001 (host) → :3000 (container)
# elan-tui runs on host: cargo run -p elan-tui -- --central-endpoint http://localhost:50051 --query-endpoint http://localhost:3001
```

Docker config files: `config/*.docker.toml`
Sample data: `data/crm/customers.csv`, `data/finance/transactions.csv`

### Local (individual services)

```bash
export DATABASE_URL="sqlite:///$(pwd)/elan_central.db"
# Terminal 1: cargo run -p elan-central -- --config config/central.toml
# Terminal 2: cargo run -p elan-coordinator -- --config config/coordinator.example.toml
# Terminal 3: cargo run -p elan-executor -- --config config/executor.toml
# Terminal 4: cargo run -p elan-query -- --config config/query.toml
# Terminal 5: cargo run -p elan-tui -- --central-endpoint http://localhost:50051 --query-endpoint http://localhost:3001
```

## Known Limitations / TODOs

- Auth tokens are bare usernames (no JWT/session validation) — replace with real auth before production
- Row-level filtering pushdown to remote executors is not yet implemented (filter runs locally after results return)
- `SessionContext::remote("df://...")` is called per-query execution; should pool connections
- No TLS on gRPC channels in the PoC (add via `tonic::transport::Channel::tls_config()`)
- `sqlx::query!` macros are NOT used anywhere (deliberate): switched entirely to runtime `sqlx::query` with `Row::try_get()` to avoid JOIN type-inference issues with sqlx 0.8 + SQLite. Compile with `DATABASE_URL` env var set.
- Column masking (REDACT/SHA256) from IAM policies is not yet applied — access is Allow/Deny only in this PoC
- Two tonic versions in the dep graph (0.12 workspace + 0.14 for elan-executor) — this is expected; do not try to unify them

## DataFusion 53 API Notes (for future maintainers)

- `PlanProperties::new()` takes 4 args: `(EquivalenceProperties, Partitioning, EmissionType, Boundedness)`
- `Boundedness` and `EmissionType` are in `datafusion_physical_plan::execution_plan` (not re-exported from `datafusion::physical_plan`) — requires `datafusion-physical-plan` as direct dependency
- `ExecutionPlan::properties()` returns `&Arc<PlanProperties>` (not `&PlanProperties`) — store as `Arc<PlanProperties>` in your struct
- `ExecutionPlan` now requires `fn name(&self) -> &str` implementation
- `TableProvider::scan()` signature: `_state: &dyn Session` (not `&SessionState`)
- `TableProvider`, `CatalogProvider`, `SchemaProvider` now require `Debug` on implementors
- `SessionContext::add_physical_optimizer_rule()` does not exist; use `SessionStateBuilder::with_physical_optimizer_rule()` then `SessionContext::new_with_state()`
- `Expr::Literal` is now a 2-tuple variant: `Expr::Literal(scalar, metadata)` — match with `Expr::Literal(scalar, _metadata)`
- Arrow cast: use `arrow_cast::cast()` (from `arrow-cast` crate), not `arrow_array::cast::cast()`
