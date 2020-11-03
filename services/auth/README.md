# Auth Service

A moderately complex authorization service that can authenticate users and clients and issue various forms of access tokens.

Supported Operations:

* Users can register with a username and password
* Clients can retrieve an auth_token and refresh_token using a username/password combination - *resource owner password credentials*
* Clients can retrieve an auth_token using a static credential and a client id - *client credentials*
* Clients can retrieve a new auth_token and refresh_token using a refresh_token and a client id - *token renewal*
* Users can have different permission levels granting different levels of access to a resource server
* Access tokens are JWTs so can be validated by a resource server without requiring an introspection request to the authorization server
* Users can change their username

## Running

The tests and server require a DynamoDB database to be running. This can be setup by running the below from the crate root

```
docker-compose up -d
```

You can then startup the authorization server

```
cargo run
```

Or run the tests

```
cargo test
```

## Example Requests

When first started it will print an admin username and password that can be used only with the loopback client, which is only supported on the loopback interface.

For example

```
$ http -f POST localhost:8080/api/v1/token grant_type=password username=admin password=4Lo7W-y73P7HxUZ4FXBfdgGb4yLhylGs client_id=loopback scope=superuser
HTTP/1.1 200 OK
content-length: 670
content-type: application/json
date: Wed, 03 Jun 2020 08:08:06 GMT

{
    "access_token": "eyJhbGciOiJSUzI1NiIsInR5cCI6IkpXVCIsImtpZCI6IjEiLCJqa3UiOiJodHRwOi8vbG9jYWxob3N0OjgwODAvLndlbGwta25vd24vandrcy5qc29uIn0.eyJleHAiOiIyMDIwLTA2LTAzVDA4OjIzOjA2LjcyNjY4N1oiLCJpYXQiOiIyMDIwLTA2LTAzVDA4OjA4OjA2LjcyNjY4N1oiLCJjaWQiOiJsb29wYmFjayIsInN1YiI6ImFkbWluX2lkIiwic2NvcGVzIjoic3VwZXJ1c2VyIn0.EsWUSdP2HIJdKYST_P63IjNizDfDjpAzdXSdKzjRh--3J26xc3rKKl9-pAU7UgAxHArYd7v2DOUngPvAoyNxrU44wRpOCzLR7Zyr-ayBP_kQ766s37mwKX8qBQi-LmqfFL9BLeIUWNWjTg0ZmtlbY4UY2jXN7wl8QfW_dPlx2rtPCbTwNH-6FftIau1AvKG9oLkr72g6ae1ySponBsdVylptyW1lSFfGtscQNLyuhliR0-U5xK1gVprl5h6dxrOympGXGJnwlgAezpwERecPHLo9JDVcS9DJ65iqpH2m8dzRLcYa0rFT0JtMUVaLlt1xC1JAdUWERVzI9Z3lvio9zQ",
    "expires_in": 900
}
```

Which then be used to query the loopback client

```
$ http localhost:8080/api/v1/client/loopback 'Authorization:eyJhbGciOiJSUzI1NiIsInR5cCI6IkpXVCIsImtpZCI6IjEiLCJqa3UiOiJodHRwOi8vbG9jYWxob3N0OjgwODAvLndlbGwta25vd24vandrcy5qc29uIn0.eyJleHAiOiIyMDIwLTA2LTAzVDA4OjIzOjA2LjcyNjY4N1oiLCJpYXQiOiIyMDIwLTA2LTAzVDA4OjA4OjA2LjcyNjY4N1oiLCJjaWQiOiJsb29wYmFjayIsInN1YiI6ImFkbWluX2lkIiwic2NvcGVzIjoic3VwZXJ1c2VyIn0.EsWUSdP2HIJdKYST_P63IjNizDfDjpAzdXSdKzjRh--3J26xc3rKKl9-pAU7UgAxHArYd7v2DOUngPvAoyNxrU44wRpOCzLR7Zyr-ayBP_kQ766s37mwKX8qBQi-LmqfFL9BLeIUWNWjTg0ZmtlbY4UY2jXN7wl8QfW_dPlx2rtPCbTwNH-6FftIau1AvKG9oLkr72g6ae1ySponBsdVylptyW1lSFfGtscQNLyuhliR0-U5xK1gVprl5h6dxrOympGXGJnwlgAezpwERecPHLo9JDVcS9DJ65iqpH2m8dzRLcYa0rFT0JtMUVaLlt1xC1JAdUWERVzI9Z3lvio9zQ'
HTTP/1.1 200 OK
content-length: 94
content-type: application/json
date: Wed, 03 Jun 2020 08:09:37 GMT

{
    "client_id": "loopback",
    "client_name": "loopback",
    "grants": [
        "password"
    ],
    "scopes": [
        "superuser"
    ]
}
```

Or create a new, non-loopback restricted client

```
$ http -v POST localhost:8080/api/v1/client client_name=foo scopes:='["superuser"]' grants:='["password", "refresh_token"]' 'Authorization:eyJhbGciOiJSUzI1NiIsInR5cCI6IkpXVCIsImtpZCI6IjEiLCJqa3UiOiJodHRwOi8vbG9jYWxob3N0OjgwODAvLndlbGwta25vd24vandrcy5qc29uIn0.eyJleHAiOiIyMDIwLTA2LTAzVDA4OjIzOjA2LjcyNjY4N1oiLCJpYXQiOiIyMDIwLTA2LTAzVDA4OjA4OjA2LjcyNjY4N1oiLCJjaWQiOiJsb29wYmFjayIsInN1YiI6ImFkbWluX2lkIiwic2NvcGVzIjoic3VwZXJ1c2VyIn0.EsWUSdP2HIJdKYST_P63IjNizDfDjpAzdXSdKzjRh--3J26xc3rKKl9-pAU7UgAxHArYd7v2DOUngPvAoyNxrU44wRpOCzLR7Zyr-ayBP_kQ766s37mwKX8qBQi-LmqfFL9BLeIUWNWjTg0ZmtlbY4UY2jXN7wl8QfW_dPlx2rtPCbTwNH-6FftIau1AvKG9oLkr72g6ae1ySponBsdVylptyW1lSFfGtscQNLyuhliR0-U5xK1gVprl5h6dxrOympGXGJnwlgAezpwERecPHLo9JDVcS9DJ65iqpH2m8dzRLcYa0rFT0JtMUVaLlt1xC1JAdUWERVzI9Z3lvio9zQ'
HTTP/1.1 200 OK
content-length: 52
content-type: application/json
date: Wed, 03 Jun 2020 08:13:50 GMT

{
    "client_id": "76fea8eb-7dab-4db3-8158-4879d1a64a98"
}
```

## JWT Schema

The JWTs issues by the authorization server have the following claims.

* `exp` - expiry date
* `iat` - issued at
* `sub` - user id - *omitted if client auth*
* `iss` - URL of authorization server
* `scope` - a space separated list of permissions this token grants

## DynamoDB Schema

Terminology is borrowed from the OAuth specification with a user corresponding to an end-user of the application and a client a particular way that a user may interact with the application, e.g. a particular website, web server, mobile app, etc... Confidential clients, e.g. backend servers, can have associated secrets they can use to authenticate either on their own, i.e. the client credential flow, or on behalf of another entity.

As per [AWS recommendations](https://docs.aws.amazon.com/amazondynamodb/latest/developerguide/bp-general-nosql-design.html#bp-general-nosql-design-concepts) a single DynamoDB table is used for the service, with composite primary keys. The Global Secondary Index is intended to allow lookups of all tokens belonging to a given user, but an endpoint for this hasn't been implemented yet.

| Entity | PK + GSI1-SK | GSI1-PK |Additional Attributes |
| --- | --- | --- | --- |
| User Record | U#User ID  | _ | Full Name |
| User Credential | UN#Username | User ID | Scopes, Hashed Credential |
| Client Record | C#Client ID | _ | Client Name, Grants, Scopes, Loopback, (Hashed Credential) |
| Renewal Token | RT#Client ID#Hashed Credential | User ID | Device Name, Scopes, Expiry |
