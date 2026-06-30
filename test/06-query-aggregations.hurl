# Transactions per category

POST http://localhost:3001/api/v1/query
Authorization: Bearer alice
Content-Type: application/json
{
  "sql": "SELECT category, COUNT(*) AS tx_count FROM elan.finance.transactions GROUP BY category ORDER BY tx_count DESC"
}

HTTP 200
[Asserts]
jsonpath "$.columns" includes "category"
jsonpath "$.columns" includes "tx_count"
jsonpath "$.rows" isCollection


# Customers per country

POST http://localhost:3001/api/v1/query
Authorization: Bearer alice
Content-Type: application/json
{
  "sql": "SELECT country, COUNT(*) AS customer_count FROM elan.crm.customers GROUP BY country ORDER BY country"
}

HTTP 200
[Asserts]
jsonpath "$.columns" includes "country"
jsonpath "$.columns" includes "customer_count"
jsonpath "$.rows" count == 10


# Transactions per currency with total volume

POST http://localhost:3001/api/v1/query
Authorization: Bearer alice
Content-Type: application/json
{
  "sql": "SELECT currency, COUNT(*) AS tx_count, SUM(amount) AS total_volume FROM elan.finance.transactions WHERE status = 'completed' GROUP BY currency ORDER BY tx_count DESC"
}

HTTP 200
[Asserts]
jsonpath "$.columns" includes "currency"
jsonpath "$.columns" includes "tx_count"
jsonpath "$.columns" includes "total_volume"


# Most active merchants

POST http://localhost:3001/api/v1/query
Authorization: Bearer alice
Content-Type: application/json
{
  "sql": "SELECT merchant, category, COUNT(*) AS purchases FROM elan.finance.transactions WHERE status = 'completed' GROUP BY merchant, category ORDER BY purchases DESC LIMIT 5"
}

HTTP 200
[Asserts]
jsonpath "$.columns" includes "merchant"
jsonpath "$.columns" includes "purchases"
