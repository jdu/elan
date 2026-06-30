# Health checks for all HTTP services

GET http://localhost:8080/health
HTTP 200
[Asserts]
jsonpath "$.status" == "ok"


GET http://localhost:3001/health
HTTP 200
[Asserts]
jsonpath "$.status" == "ok"


GET http://localhost:8081/health
HTTP 200
[Asserts]
jsonpath "$.status" == "ok"
