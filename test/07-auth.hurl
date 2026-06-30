# Auth: anonymous user — treated as user "anonymous"
# In a freshly seeded system with no IAM policies, queries from anonymous
# will return empty results (default-deny) rather than an error.

POST http://localhost:3001/api/v1/query
Content-Type: application/json
{
  "sql": "SELECT * FROM elan.crm.customers LIMIT 1"
}

HTTP 200
[Asserts]
jsonpath "$.query_id" exists


# Auth: named user in the Bearer token

POST http://localhost:3001/api/v1/query
Authorization: Bearer bob
Content-Type: application/json
{
  "sql": "SELECT * FROM elan.crm.customers LIMIT 1"
}

HTTP 200
[Asserts]
jsonpath "$.query_id" exists


# Coordinator auth-check endpoint
# Used by elan-executor to verify dataset access for a given caller

GET http://localhost:8081/auth/check?dataset=transactions&caller=alice
HTTP 200
[Asserts]
jsonpath "$.allowed" == true
jsonpath "$.reason" exists


GET http://localhost:8081/auth/check?dataset=customers&caller=bob
HTTP 200
[Asserts]
jsonpath "$.allowed" == true


# Missing required query parameters — coordinator should return an error

GET http://localhost:8081/auth/check
HTTP 400
