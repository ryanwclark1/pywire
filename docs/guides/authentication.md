# Authentication

`serve(auth=..., auth_method=...)` picks how clients prove who they are. The
startup handshake runs before any of your query code.

| `auth_method` | `auth=` | What the server stores |
| --- | --- | --- |
| `"trust"` (or no `auth`) | ignored | nothing: every client is accepted |
| `"cleartext"` (default with `auth`) | `AuthSource` | the password |
| `"scram-sha-256"` | `AuthSource` | the salted password (PBKDF2 output) and salt |
| `"scram-sha-256-verifier"` | `ScramVerifierSource` | PostgreSQL `rolpassword` verifiers only |

Cleartext sends the password over the wire, so only use it together with
[TLS](tls.md). Prefer SCRAM.

## SCRAM with salted passwords

```python
--8<-- "examples/auth_scram.py"
```

`get_password` returns `Password(salted_password, salt=salt)`. The PBKDF2
iteration count must equal `serve(scram_iterations=...)` (default 4096,
which is also the minimum).

## SCRAM with stored verifiers

PostgreSQL itself stores neither passwords nor salted passwords, only
`SCRAM-SHA-256$<iterations>:<salt>$<StoredKey>:<ServerKey>`. A leak of
those can't be used to log in directly. pywire checks the client's proof
against such a verifier in constant time:

```python
--8<-- "examples/scram_verifier.py"
```

Returning `None` for an unknown user runs the exchange against a mock
verifier derived from a per-server secret. The client fails at the same step
with the same `28P01` error as a wrong password, so attackers can't probe
which users exist. Each verifier carries its own salt and iteration count.

## `LoginInfo`

Both sources receive a `LoginInfo`:

| Field | Source |
| --- | --- |
| `user`, `database` | startup parameters |
| `host` | client IP address |
| `sni_server_name` | TLS SNI, when the client sent one |
| `client_certificates` | verified client chain (DER, leaf first) with mutual TLS |

## Channel binding

On TLS connections the SCRAM methods also advertise
`SCRAM-SHA-256-PLUS` (`tls-server-end-point` channel binding), which ties the
authentication to the TLS session and defeats man-in-the-middle relays.
libpq uses it automatically with `channel_binding=prefer` (the default) or
`require`.

## Rejecting a login

Raise `InvalidPassword(user)` (or any `AuthError`) from your source. To
reject *after* authentication, for example a user that may not use a
database, raise from `SessionFactory.open`.
