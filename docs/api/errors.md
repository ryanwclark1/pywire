# Errors

`pywire.errors` mirrors `pgwire::error::PgWireError` one variant to one Python
exception class, plus the `ErrorInfo` field set that PostgreSQL uses to
carry error metadata across the wire.

## Hierarchy

The classes form a three-level hierarchy:

```
Error
├── ProtocolError        # wire-protocol-level violations
│   ├── UnsupportedProtocolVersion
│   ├── InvalidCancelRequest
│   ├── InvalidMessageType
│   ├── MessageTooLarge
│   ├── InvalidTargetType
│   ├── InvalidTransactionStatus
│   ├── InvalidSSLRequestMessage
│   ├── InvalidGssEncRequestMessage
│   ├── InvalidStartupMessage
│   ├── InvalidAuthenticationMessageCode
│   ├── InvalidSecretKey
│   ├── NotReadyForQuery
│   └── InvalidOptionValue
├── AuthError            # authentication / SASL / SCRAM / OAuth
│   ├── FailedToCoercePasswordMessage
│   ├── InvalidSASLState
│   ├── UnsupportedSASLAuthMethod
│   ├── InvalidScramMessage
│   ├── InvalidPassword
│   ├── UnsupportedCertificateSignatureAlgorithm
│   ├── UserNameRequired
│   ├── InvalidOauthMessage
│   ├── OAuthAuthenticationFailed
│   ├── OAuthValidationError
│   └── OauthAuthzIdError
├── PortalNotFound
├── PortalNotStarted
├── StatementNotFound
├── ParameterIndexOutOfBound
├── InvalidRustTypeForParameter
├── FailedToParseParameter
├── QueryCanceled
├── ApiError
└── UserError
```

`pgwire`'s `IoError` variant does not appear here — it is flattened to
Python's built-in [`OSError`](https://docs.python.org/3/library/exceptions.html#OSError)
because that matches the exception callers will already be catching
around socket code.

## Catching subsets

Catch all pywire errors:

```python
import pywire

try:
    ...
except pywire.errors.Error as exc:
    ...
```

Catch only protocol-decoding failures:

```python
try:
    ...
except pywire.errors.ProtocolError as exc:
    ...
```

Catch a single variant:

```python
try:
    ...
except pywire.errors.InvalidPassword as exc:
    ...
```

## `ErrorInfo`

`ErrorInfo` carries the field set defined by the
[PostgreSQL ErrorResponse / NoticeResponse protocol](https://www.postgresql.org/docs/current/protocol-error-fields.html).
Construct one to attach structured error information to a `UserError`
that your handler raises:

```python
info = pywire.errors.ErrorInfo(
    severity="ERROR",
    code="22000",
    message="invalid value",
    detail="value 42 is out of range",
    hint="use a value between 0 and 10",
)
```

Required: `severity`, `code`, `message`. All other fields are keyword-only
optionals. Field names mirror upstream pgwire exactly.

## Reference

::: pywire.errors
    options:
      show_source: false
      heading_level: 3
      members_order: source
