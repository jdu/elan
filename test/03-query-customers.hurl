# Scan all customers

POST http://localhost:3001/api/v1/query
Authorization: Bearer alice
Content-Type: application/json
{
  "sql": "SELECT * FROM elan.crm.customers ORDER BY customer_id"
}

HTTP 200
[Asserts]
jsonpath "$.query_id" exists
jsonpath "$.columns" includes "customer_id"
jsonpath "$.columns" includes "name"
jsonpath "$.columns" includes "country"
jsonpath "$.columns" includes "tier"
jsonpath "$.rows" isCollection
jsonpath "$.rows" count == 10
jsonpath "$.duration_ms" >= 0


# Filter to gold-tier customers only

POST http://localhost:3001/api/v1/query
Authorization: Bearer alice
Content-Type: application/json
{
  "sql": "SELECT customer_id, name, country FROM elan.crm.customers WHERE tier = 'gold' ORDER BY name"
}

HTTP 200
[Asserts]
jsonpath "$.query_id" exists
jsonpath "$.columns" includes "customer_id"
jsonpath "$.rows" count == 4


# Customers from Europe (GB, DE, SE)

POST http://localhost:3001/api/v1/query
Authorization: Bearer alice
Content-Type: application/json
{
  "sql": "SELECT name, country, tier FROM elan.crm.customers WHERE country IN ('GB', 'DE', 'SE') ORDER BY country"
}

HTTP 200
[Asserts]
jsonpath "$.rows" count == 3
