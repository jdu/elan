# Scan all transactions

POST http://localhost:3001/api/v1/query
Authorization: Bearer alice
Content-Type: application/json
{
  "sql": "SELECT * FROM elan.finance.transactions ORDER BY transaction_id"
}

HTTP 200
[Asserts]
jsonpath "$.query_id" exists
jsonpath "$.columns" includes "transaction_id"
jsonpath "$.columns" includes "customer_id"
jsonpath "$.columns" includes "amount"
jsonpath "$.columns" includes "status"
jsonpath "$.rows" count == 15


# Completed transactions only

POST http://localhost:3001/api/v1/query
Authorization: Bearer alice
Content-Type: application/json
{
  "sql": "SELECT transaction_id, customer_id, amount, currency, status FROM elan.finance.transactions WHERE status = 'completed' ORDER BY transaction_id"
}

HTTP 200
[Asserts]
jsonpath "$.rows" count == 13


# Electronics purchases

POST http://localhost:3001/api/v1/query
Authorization: Bearer alice
Content-Type: application/json
{
  "sql": "SELECT transaction_id, merchant, amount, currency FROM elan.finance.transactions WHERE category = 'electronics' ORDER BY amount DESC"
}

HTTP 200
[Asserts]
jsonpath "$.columns" includes "merchant"
jsonpath "$.columns" includes "amount"


# Non-completed (pending or failed) transactions

POST http://localhost:3001/api/v1/query
Authorization: Bearer alice
Content-Type: application/json
{
  "sql": "SELECT transaction_id, customer_id, amount, currency, status FROM elan.finance.transactions WHERE status != 'completed'"
}

HTTP 200
[Asserts]
jsonpath "$.rows" count == 2
