# Elan — Federated Query System

Proof-of-concept for federated querying across multiple organisations' data stores using Apache DataFusion.

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
      ┌─────────▼──────────┐   uploads    ┌───────────────────▼──────┐
      │   elan-coordinator │ ────────────►│   MinIO (object store)   │
      │  • Reads TOML cfg  │  remote env  │  • S3-compatible API     │
      │  • Infers schemas  │              │  • Holds all dataset files│
      │  • Registers with  │              └───────────┬──────────────┘
      │    elan-central    │                          │ reads
      └────────────────────┘              ┌───────────▼──────────────┐
                                          │   elan-executor (×N)     │
                                          │  • HTTP SQL service       │
                                          │    POST /sql → Arrow IPC  │
                                          │  • DataFusion reads MinIO │
                                          └───────────┬──────────────┘
                                                      │
                                          ┌───────────▼──────────────┐
                                          │   nginx load balancer    │
                                          │  • Round-robin across    │
                                          │    executor replicas     │
                                          └───────────┬──────────────┘
                                                      │ POST /sql
      ┌────────────────────────────┐      ┌───────────▼──────────────┐
      │       elan-tui             │◄────►│     elan-query           │
      │  • SQL editor (ratatui)    │ HTTP │  • Custom catalog (gRPC) │
      │  • Results table view      │      │  • IAM enforcement       │
      │  • Audit event stream      │      │  • RemoteTableScanExec   │
      │  • Catalog browser         │      │  • HTTP SQL fan-out      │
      └────────────────────────────┘      └──────────────────────────┘
                     ▲                               │
                     │ gRPC stream_audit_events      │ gRPC publish_event
                     └───────────── elan-central ◄──┘
```

## Services

| Crate / Service      | Port(s)                         | Role |
|---|---|---|
| `elan-central`       | 50051 (gRPC) / 8080 (HTTP)      | Central catalog & IAM authority |
| `elan-coordinator`   | 8081 (HTTP)                     | Remote coordinator — uploads data, registers datasets |
| `elan-executor` ×N   | 50056 (HTTP SQL, internal)      | Remote query engine — reads from MinIO, scalable pool |
| nginx                | 50056 (external)                | Load balancer in front of executor pool |
| MinIO                | 9000 (S3 API) / 9001 (console)  | S3-compatible object store for shared dataset files |
| `elan-query`         | 3000 (HTTP)                     | Main DataFusion query service |
| `elan-tui`           | —                               | Terminal UI |

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

**Never change datafusion or object_store independently.** DataFusion 53 pins `object_store = "0.13"` — if you add `object_store` as a direct dep it must use the same minor version or Cargo will complain.

- `datafusion = "53"` (53.1.0) + `datafusion-physical-plan = "53"`
- `object_store = "0.13"` (must match datafusion 53's transitive pin)
- `ballista = "53.0.0"` + `ballista-executor = "53"` + `ballista-scheduler = "53"` (kept as deps but not used for query dispatch — see Ballista note below)
- `ballista-core = "53.0.0"`
- `arrow-* = "58"` (derived from datafusion 53)
- `tonic = "0.12"` for workspace gRPC services; `elan-executor` overrides to `tonic = "0.14"` to match `ballista-executor`/`ballista-core` internals
- `tui-textarea = "0.7"` (features = ["ratatui"]) to match `ratatui = "0.29"`

## Workspace Layout

```
elan/
├── Cargo.toml              # workspace; all version pinning lives here
├── AGENTS.md               # this file
├── README.md               # technical README
├── TOUR.md                 # stakeholder-friendly walkthrough
├── TODO.md                 # known gaps and planned work
├── proto/                  # Protobuf definitions for all gRPC services
│   ├── catalog.proto
│   ├── coordinator.proto
│   ├── iam.proto
│   └── audit.proto
├── migrations/             # SQLite schema migrations (sqlx)
├── nginx/
│   └── executor.conf       # nginx load balancer config for executor pool
├── config/
│   ├── *.docker.toml       # Docker Compose configs (reference minio, s3:// paths)
│   └── coordinator.example.toml
├── data/                   # Sample CSV files (local source; coordinator uploads to MinIO)
│   ├── crm/customers.csv
│   └── finance/transactions.csv
└── crates/
    ├── elan-common/        # Shared types, errors, proto re-exports
    ├── elan-iam/           # IAM/RBAC engine + DataFusion optimizer rule
    ├── elan-audit/         # Audit event sink (CentralAuditSink → elan-central gRPC)
    ├── elan-central/       # Central catalog + IAM gRPC service
    ├── elan-coordinator/   # Remote coordinator: schema inference, MinIO upload, registration
    ├── elan-executor/      # HTTP SQL service + DataFusion + MinIO object store
    ├── elan-query/         # Main DataFusion query service
    └── elan-tui/           # Terminal UI
```

## Data Catalog (SQLite — elan-central)

Tables: `coordinators`, `datasets`, `iam_subjects`, `iam_group_members`, `iam_policies`, `audit_events`

See `migrations/` for full schema.

## Object Storage (MinIO)

Dataset files live in MinIO, not on the executor's local filesystem. This enables the executor pool to scale horizontally — all replicas read from the same bucket.

**Startup sequence:**
1. `minio-init` (Docker one-shot) creates the `elan-data` bucket.
2. `elan-coordinator` uploads each configured file to MinIO: key = `{namespace}/{filename}`, e.g. `crm/customers.csv`.
3. `elan-executor` registers an `AmazonS3Builder` object store with DataFusion keyed to `s3://elan-data/`, then registers tables using `s3://elan-data/...` paths.

**Config:** `[object_store]` section in both coordinator and executor TOML configs:
```toml
[object_store]
endpoint   = "http://minio:9000"
access_key = "minioadmin"
secret_key = "minioadmin"
bucket     = "elan-data"
allow_http = true           # plain HTTP for local MinIO; disable in production
```

**Scaling:** `docker compose up --scale elan-executor=N` — nginx resolves the `elan-executor` service name via Docker's embedded DNS (`127.0.0.11`) and round-robins across all replicas. New replicas are picked up on the next DNS resolution without restarting nginx.

## IAM/RBAC Design

Enforcement happens in two layers inside `elan-query`:

1. **Catalog filter** (`elan-iam/src/catalog_filter.rs`): `SchemaProvider::table()` returns `None` for datasets the user cannot see — hides table existence.
2. **Physical optimizer rule** (`elan-iam/src/optimizer.rs`): After logical planning, `IamFilterRule` traverses the plan tree:
   - Finds `RemoteTableScanExec` nodes
   - Calls `IamEngine::check(subject, resource, "SELECT")`
   - Deny → replaces node with `EmptyExec` (preserves schema, zero rows)
   - Allow with row filter → prepends `FilterExec` over the scan
   - Allow clean → passes through

Column masking is parsed from policies but not yet applied in the optimizer (see TODO.md).

## Query Execution (HTTP SQL service)

`elan-query`'s `RemoteTableScanExec::execute()` POSTs SQL to the executor pool via nginx:

```
POST http://elan-executor-lb:50056/sql
Body: SELECT * FROM "customers" WHERE tier = 'gold'
Response: Arrow IPC stream bytes
```

`execute()` uses `tokio::task::block_in_place` + `Handle::current().block_on()` because DataFusion's `ExecutionPlan::execute()` is synchronous but making the HTTP call requires async. The response is decoded with `arrow_ipc::reader::StreamReader`. The actual batch schema from the response is used for `MemoryStream` (not the potentially-stale schema stored in elan-central — that schema is used only for query planning).

SQL is always `SELECT *` (no column projection pushed down to the executor) because the stored schema in elan-central may be a stale placeholder. Projection can be added once schema freshness is guaranteed.

## Audit Events

`elan-query` publishes audit events directly to `elan-central` via gRPC (`AuditService/PublishEvent`) using `CentralAuditSink`. elan-central stores them in SQLite and broadcasts to all connected TUI clients via `AuditService/StreamAuditEvents`.

Kafka infrastructure is still present in docker-compose (and `KafkaAuditSink` exists in `elan-audit`) but is no longer wired into the query path.

## Ballista Note

`ballista`, `ballista-executor`, and `ballista-scheduler` remain as dependencies of `elan-executor` but are not used for query dispatch. The Ballista scheduler starts on port 50055 but receives no queries. All execution goes through the HTTP SQL service on port 50056.

The reason Ballista was removed from the query path: `SessionContext::remote()` plans SQL client-side (needs schemas), and Ballista serialises `TableProvider` data into the logical plan rather than looking up real file providers on the workers. With datasets now in MinIO, Ballista distributed execution could be re-enabled by pointing it directly at `s3://` file paths (which are serialisable in `CsvExec`/`ParquetExec`) — this is the natural next step for within-environment parallelism.

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
- `AuditService` — PublishEvent, StreamAuditEvents (server stream to TUI)

### elan-coordinator (HTTP on :8081)

```
GET  /health
GET  /auth/check?dataset=<name>&caller=<user>  → { allowed: bool, reason: str }
```

### elan-executor (HTTP on :50056)

```
POST /sql    Body: raw SQL text    → Arrow IPC stream bytes
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

## Running the PoC

### Docker (all services)

```bash
docker compose build
docker compose down && docker compose up

# Scale the executor pool
docker compose up --scale elan-executor=3

# Endpoints
# elan-central  :50051 (gRPC) / :8080 (HTTP health)
# elan-query    :3001 (host) → :3000 (container)
# MinIO S3 API  :9000
# MinIO console :9001  (minioadmin / minioadmin)

# TUI (run on host)
cargo run -p elan-tui -- --central-endpoint http://localhost:50051 --query-endpoint http://localhost:3001
```

### Local (individual services)

```bash
export DATABASE_URL="sqlite:///$(pwd)/elan_central.db"
# Terminal 1: cargo run -p elan-central -- --config config/central.toml
# Terminal 2: cargo run -p elan-coordinator -- --config config/coordinator.example.toml
# Terminal 3: cargo run -p elan-executor -- --config config/executor.toml
# Terminal 4: cargo run -p elan-query -- --config config/query.toml
# Terminal 5: cargo run -p elan-tui -- --central-endpoint http://localhost:50051 --query-endpoint http://localhost:3001
```

Note: local config (`config/executor.toml`, `config/coordinator.example.toml`) uses local file paths, not S3 URLs. Remove the `[object_store]` section to run without MinIO.

## Known Limitations / TODOs

See TODO.md for the full list. Key items:

- Auth tokens are bare usernames — replace with JWT/OIDC before production
- Column masking from IAM policies is parsed but not applied
- Ballista distributed execution within the executor pool is not yet wired up (MinIO storage is in place; needs `CsvExec`/`ParquetExec` plan path)
- No TLS on gRPC or HTTP in the PoC
- `sqlx::query!` macros are not used (deliberate): switched to runtime `sqlx::query` with `Row::try_get()` to avoid JOIN type-inference issues with sqlx 0.8 + SQLite
- Two tonic versions in the dep graph (0.12 workspace + 0.14 for elan-executor) — intentional, do not try to unify

## DataFusion 53 API Notes

- `PlanProperties::new()` takes 4 args: `(EquivalenceProperties, Partitioning, EmissionType, Boundedness)`
- `Boundedness` and `EmissionType` are in `datafusion_physical_plan::execution_plan` — requires `datafusion-physical-plan` as a direct dependency
- `ExecutionPlan::properties()` returns `&Arc<PlanProperties>` — store as `Arc<PlanProperties>` in your struct
- `ExecutionPlan` requires `fn name(&self) -> &str`
- `TableProvider::scan()` signature: `_state: &dyn Session` (not `&SessionState`)
- `TableProvider`, `CatalogProvider`, `SchemaProvider` require `Debug` on implementors
- `SessionContext::add_physical_optimizer_rule()` does not exist; use `SessionStateBuilder::with_physical_optimizer_rule()` then `SessionContext::new_with_state()`
- `Expr::Literal` is a 2-tuple variant: `Expr::Literal(scalar, metadata)` — match with `Expr::Literal(scalar, _metadata)`
- Arrow cast: use `arrow_cast::cast()` (from `arrow-cast` crate), not `arrow_array::cast::cast()`

## object_store 0.13 API Notes

- `ObjectStore::put()` takes `PutPayload`, not `Bytes` directly — use `PutPayload::from(vec)` or `PutPayload::from_bytes(bytes)`
- The `put` convenience method lives on `ObjectStoreExt` (blanket impl) — import `use object_store::ObjectStoreExt`
- Register with DataFusion: `ctx.register_object_store(&Url::parse("s3://bucket/")?, Arc::new(store))`
- MinIO requires `.with_region("us-east-1")` even though it ignores the value; omitting it causes a builder error
