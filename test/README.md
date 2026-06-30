# elan test suite

HTTP integration tests written in [Hurl](https://hurl.dev/). Each file is a numbered sequence of requests with assertions.

## Prerequisites

```bash
# Install hurl (https://hurl.dev/docs/installation.html)
curl -LO https://github.com/Orange-OpenSource/hurl/releases/latest/download/hurl-x86_64-unknown-linux-gnu.tar.gz
tar -xzf hurl-x86_64-unknown-linux-gnu.tar.gz
sudo mv hurl /usr/local/bin/

# Or via cargo
cargo install hurl
```

All tests assume the full stack is running locally (via Docker or individual processes). See the root `README.md` for startup instructions.

## Run all tests

```bash
hurl --test test/*.hurl
```

## Run a single file

```bash
hurl --test test/03-query-customers.hurl
```

## Run with verbose output (shows full request/response)

```bash
hurl --test --verbose test/05-query-joins.hurl
```

## Files

| File | What it covers |
|---|---|
| `01-health.hurl` | Health endpoints on all three HTTP services |
| `02-catalog.hurl` | `GET /api/v1/catalog` — dataset listing |
| `03-query-customers.hurl` | Scans and filters on `crm.customers` |
| `04-query-transactions.hurl` | Scans and filters on `finance.transactions` |
| `05-query-joins.hurl` | Cross-namespace SQL joins (the main federated query use-case) |
| `06-query-aggregations.hurl` | GROUP BY, COUNT, SUM across both namespaces |
| `07-auth.hurl` | Bearer token handling, coordinator auth-check endpoint |
| `08-edge-cases.hurl` | Bad SQL, missing tables, empty results, missing body |

## Notes

- The `Authorization: Bearer <username>` header sets the IAM subject. With a fresh database (no policies seeded), the IAM default-deny means most queries return empty results — add policies via `grpcurl` as described in the root README to get data back.
- Row counts in the assertions match the sample data in `data/`. If you add rows to the CSV files, update the `count ==` assertions accordingly.
- `07-auth.hurl` expects the coordinator's missing-parameter response to be HTTP 400. If axum returns 422 for missing query params instead, update that assertion.
