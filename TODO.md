# elan — TODO

Items are grouped by component. POC shortcuts are marked **[poc shortcut]** — things that exist in a placeholder/stub form and need real implementations before production use.

---

## Query execution

- **Projection pushdown** — `RemoteTableScanExec` always sends `SELECT *` to the executor because the schema stored in elan-central was historically a placeholder. Now that the coordinator infers real schemas, we can project only the columns needed by the query plan and push that down into the SQL sent to the executor.
- **Predicate pushdown** — simple `WHERE` clauses are already appended to the executor SQL, but this is done manually by inspecting `TableProviderFilterPushDown`. Complex expressions (e.g. `IN`, `BETWEEN`, `LIKE`) may not be fully represented. Verify coverage and fill gaps.
- **Result pagination** — `POST /api/v1/query` currently returns all rows in one response. Add `limit` / `offset` or cursor-based pagination for large result sets.
- **Query timeouts** — no per-query deadline is enforced. Add a configurable timeout that cancels both the local DataFusion plan and the in-flight HTTP request to the executor.
- **Cross-executor join strategy** — joins across datasets on different executors pull all rows to elan-query and join locally. For large datasets this is impractical. Options: broadcast-hash join for small dimensions, or a shuffle-based approach where one executor streams to another.
- **Bytes scanned telemetry** — `bytes_scanned` in the audit event is hardcoded to `0`. Wire up actual byte counts from the executor response (can be a response header).

---

## IAM / access control

- **Column masking** **[poc shortcut]** — `Policy` has a `column_mask_json` field and the IAM engine parses it into an `AccessDecision`, but the physical optimizer rule (`IamFilterRule`) never acts on it. Add a `ProjectionExec` that replaces masked columns with `NULL` or a redaction expression.
- **IAM policy refresh in elan-query** — policies are loaded once at startup and never refreshed. Changes made via `IamService/CreatePolicy` or `DeletePolicy` won't take effect until elan-query restarts. Add the same 30-second refresh loop used for the dataset catalog.
- **Group-based policies** — `SubjectType::Group` is modelled in the type system but nothing populates `group_ids` from the bearer token. Decide on the group claim format (JWT group claim, LDAP, etc.) and wire it up through `Subject`.
- **Real authentication** **[poc shortcut]** — `Authorization: Bearer <username>` treats the token value as the user ID. Replace with JWT validation (HS256 / RS256) and extract `sub`, `groups`, and other claims from the token payload.
- **Executor HTTP endpoint is unauthenticated** **[poc shortcut]** — `POST /sql` on port 50056 accepts any request. In production, elan-query should present a shared secret or mTLS certificate that the executor verifies. Without this, anything that can reach the executor port can query data without IAM checks.

---

## Coordinator

- **Postgres schema inference** **[poc shortcut]** — `DatasetConfig::Postgres` returns a `_placeholder` schema. Connect to the database, run `SELECT * FROM <schema>.<table> LIMIT 0`, and extract the column types.
- **Delta Lake schema inference** **[poc shortcut]** — `DatasetConfig::Delta` returns a `_placeholder` schema. Read the Delta log to extract the schema without scanning data.
- **Dataset re-registration on config change** — the coordinator only registers datasets at startup. If a new `[[datasets]]` block is added to the config, the coordinator must be restarted. Watch the config file for changes and re-register incrementally (add new, update changed, deregister removed).
- **Dataset deduplication in elan-central** — each coordinator restart creates a new UUID for each dataset via `Uuid::new_v4()`. Over time, elan-central accumulates duplicate registrations. Use a stable ID derived from `(coordinator_id, namespace, name)` and upsert instead of insert.
- **Coordinator auth check is a stub** **[poc shortcut]** — `GET /health/auth-check` on the coordinator always returns `allowed: true`. This endpoint is intended to let the executor verify per-row access before serving data. Implement real IAM evaluation here or remove the indirection and have the executor call elan-central's IAM gRPC directly.

---

## Executor

- **Remove Ballista** — the Ballista scheduler and executor start on port 50055 but are never called. All query dispatch now goes through the HTTP SQL service on port 50056. Remove `ballista-executor`, `ballista-scheduler`, and `datafusion-proto` from `elan-executor/Cargo.toml` to reduce compile time and binary size.
- **TLS on HTTP SQL service** — the `POST /sql` endpoint runs plain HTTP. Add TLS (rustls) so traffic between elan-query and remote executors is encrypted in transit.
- **Postgres and Delta source execution** — `elan-executor` registers datasets via DataFusion's CSV and Parquet readers. Postgres and Delta table types are defined in the coordinator config but the executor has no corresponding `register_postgres` / `register_delta` implementation.
- **Streaming results** — the executor collects all Arrow batches into memory before sending. For large scans, stream Arrow IPC batches back as they are produced instead of buffering everything.

---

## elan-central

- **Coordinator liveness from heartbeat** — the `last_heartbeat_at` and `is_alive` columns exist in the `coordinators` table and the heartbeat stream is implemented, but elan-query's catalog read doesn't filter out datasets whose coordinator has gone silent. Mark coordinators as dead after a missed heartbeat window and exclude their datasets from query planning.
- **Dataset deregistration** — there is no `DeregisterDataset` RPC. When a coordinator removes a dataset from its config or shuts down permanently, the orphaned dataset record remains in SQLite and elan-query will attempt to query an executor that no longer serves it.
- **Audit event `occurred_at` field** — `AuditEventProto.occurred_at` is always `None` when published from elan-query. Populate the `Timestamp` field with the actual event time.

---

## Web UI

- **Replace or supplement the TUI with a web interface** — the TUI is useful for development but the intended end-user surface is a browser-based query builder adjacent to elan-central. The elan-query HTTP API is already the right shape; a web UI would call `POST /api/v1/query` and `GET /api/v1/catalog` directly.
- **Catalog explorer** — a tree view of namespaces → datasets → columns (with types) so users can browse available data without knowing the schema upfront.

---

## Operations / productionisation

- **TLS on gRPC** — all inter-service gRPC (elan-central ↔ elan-query, elan-central ↔ elan-coordinator) runs plaintext. Add server-side TLS at minimum; mTLS for executor-to-central calls.
- **Metrics** — no Prometheus metrics are exported. Instrument query latency, bytes scanned, IAM deny rate, catalog refresh errors, and executor HTTP response times.
- **Kubernetes / Helm** — no deployment manifests exist. Central components (elan-central, elan-query) suit a standard Deployment + Service + PVC. Remote components (elan-coordinator, elan-executor) are better modelled as a DaemonSet or single-replica Deployment per remote environment with the data volume mounted.
- **Config hot-reload in elan-query** — beyond the 30-second catalog refresh from elan-central, elan-query's own config file (HTTP address, central endpoint, instance name) requires a restart to change.
- **SQLite → Postgres for elan-central** — SQLite is fine for a PoC but becomes a bottleneck under concurrent catalog writes. Make the store backend pluggable so elan-central can be pointed at Postgres for production.

---

## Testing

- **Integration tests** — no tests verify the end-to-end path (coordinator registers → elan-query plans → executor returns data). Add a test harness that spins up in-process instances and asserts on query results.
- **IAM unit tests** — `IamFilterRule` and `SnapshotIamEngine` have logic for deny, row filter, and column mask decisions. Add table-driven tests covering all three branches including wildcard subjects and resource pattern matching.
- **Schema inference tests** — add test fixtures (a small CSV and a small Parquet file) and assert that `infer_schema` in the coordinator produces the expected Arrow schema.
