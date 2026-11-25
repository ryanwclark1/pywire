# TLS

Pass `tls=TLSConfig(cert, key)` to `serve` to accept PostgreSQL's
`SSLRequest` upgrade. `cert` is a PEM certificate chain and `key` a PEM
private key.

```python
--8<-- "examples/tls.py"
```

## Options

| `TLSConfig` field | Default | Effect |
| --- | --- | --- |
| `cert`, `key` | (required) | server certificate chain and private key paths |
| `require` | `True` | refuse clients that do not upgrade to TLS |
| `client_ca` | `None` | PEM CA bundle; when set, clients **must** present a certificate chaining to it (mutual TLS) |

With `require=False`, TLS is offered but plaintext clients are still
accepted. Check `LoginInfo.sni_server_name` if you need to know which name a
client asked for.

## Mutual TLS

With `client_ca`, the handshake fails unless the client presents a valid
certificate, as with `sslcert=` and `sslkey=` in libpq. The verified chain is
available as `LoginInfo.client_certificates` (DER bytes, leaf first), so an
`AuthSource` or `SessionFactory` can map the certificate subject to a user:

```python
from cryptography import x509

leaf = x509.load_der_x509_certificate(login.client_certificates[0])
common_name = leaf.subject.rfc4514_string()
```

## Security notes

- pgwire 0.42 refuses plaintext bytes pipelined between `SSLRequest` and the
  TLS handshake (GHSA-95f6-gpr7-g65r).
- The server negotiates the `postgresql` ALPN protocol, so PostgreSQL 17+
  clients can use direct TLS (`sslnegotiation=direct`).
- TLS enables SCRAM channel binding (`SCRAM-SHA-256-PLUS`); see
  [Authentication](authentication.md).
