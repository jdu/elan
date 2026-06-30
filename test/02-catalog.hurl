# List datasets visible to the requesting user via the catalog endpoint

GET http://localhost:3001/api/v1/catalog
Authorization: Bearer alice
HTTP 200
[Asserts]
jsonpath "$.namespaces" isCollection
jsonpath "$.namespaces[0].name" exists
