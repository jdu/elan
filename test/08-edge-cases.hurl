# Syntax error — DataFusion should reject with 500

POST http://localhost:3001/api/v1/query
Authorization: Bearer alice
Content-Type: application/json
{
  "sql": "SELECT * FORM elan.crm.customers"
}

HTTP 500
[Asserts]
jsonpath "$.error" exists


# Reference a table that doesn't exist — should return 500

POST http://localhost:3001/api/v1/query
Authorization: Bearer alice
Content-Type: application/json
{
  "sql": "SELECT * FROM elan.crm.nonexistent_table"
}

HTTP 500
[Asserts]
jsonpath "$.error" exists


# Reference a namespace that doesn't exist

POST http://localhost:3001/api/v1/query
Authorization: Bearer alice
Content-Type: application/json
{
  "sql": "SELECT * FROM elan.ops.orders"
}

HTTP 500
[Asserts]
jsonpath "$.error" exists


# Empty result set (no rows match)

POST http://localhost:3001/api/v1/query
Authorization: Bearer alice
Content-Type: application/json
{
  "sql": "SELECT * FROM elan.crm.customers WHERE country = 'XX'"
}

HTTP 200
[Asserts]
jsonpath "$.query_id" exists
jsonpath "$.rows" count == 0
jsonpath "$.columns" isCollection


# LIMIT 0

POST http://localhost:3001/api/v1/query
Authorization: Bearer alice
Content-Type: application/json
{
  "sql": "SELECT * FROM elan.finance.transactions LIMIT 0"
}

HTTP 200
[Asserts]
jsonpath "$.rows" count == 0


# Missing request body — should return 400 or 422

POST http://localhost:3001/api/v1/query
Authorization: Bearer alice
Content-Type: application/json

HTTP 422


# Session ID is optional — verify it is accepted when provided

POST http://localhost:3001/api/v1/query
Authorization: Bearer alice
Content-Type: application/json
{
  "sql": "SELECT 1 AS one",
  "session_id": "test-session-abc123"
}

HTTP 200
[Asserts]
jsonpath "$.query_id" exists
