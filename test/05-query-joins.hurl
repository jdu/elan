# Cross-namespace join: total spend per customer

POST http://localhost:3001/api/v1/query
Authorization: Bearer alice
Content-Type: application/json
{
  "sql": "SELECT c.name, c.country, c.tier, COUNT(t.transaction_id) AS tx_count, SUM(t.amount) AS total_amount FROM elan.finance.transactions t JOIN elan.crm.customers c ON t.customer_id = c.customer_id WHERE t.status = 'completed' GROUP BY c.name, c.country, c.tier ORDER BY total_amount DESC"
}

HTTP 200
[Asserts]
jsonpath "$.query_id" exists
jsonpath "$.columns" includes "name"
jsonpath "$.columns" includes "total_amount"
jsonpath "$.rows" isCollection


# Cross-namespace join: failed or pending transactions with customer details

POST http://localhost:3001/api/v1/query
Authorization: Bearer alice
Content-Type: application/json
{
  "sql": "SELECT t.transaction_id, c.name, c.email, t.amount, t.currency, t.merchant, t.status FROM elan.finance.transactions t JOIN elan.crm.customers c ON t.customer_id = c.customer_id WHERE t.status != 'completed' ORDER BY t.transaction_id"
}

HTTP 200
[Asserts]
jsonpath "$.columns" includes "name"
jsonpath "$.columns" includes "email"
jsonpath "$.columns" includes "status"
jsonpath "$.rows" count == 2


# Cross-namespace join: spend by tier

POST http://localhost:3001/api/v1/query
Authorization: Bearer alice
Content-Type: application/json
{
  "sql": "SELECT c.tier, COUNT(DISTINCT c.customer_id) AS customers, COUNT(t.transaction_id) AS transactions FROM elan.crm.customers c LEFT JOIN elan.finance.transactions t ON c.customer_id = t.customer_id GROUP BY c.tier ORDER BY c.tier"
}

HTTP 200
[Asserts]
jsonpath "$.columns" includes "tier"
jsonpath "$.columns" includes "customers"
jsonpath "$.columns" includes "transactions"
