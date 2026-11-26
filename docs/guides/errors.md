# Errors

Clients see errors as PostgreSQL `ErrorResponse` messages: a severity, a
five-character **SQLSTATE** code that drivers map to exception classes, a
message, and optional fields such as `detail`, `hint` and `constraint`.
pywire gives you three ways to produce one.

```python
--8<-- "examples/error_handling.py"
```

| You do | Client gets | Scope |
| --- | --- | --- |
| `raise SomeError("...")` (a `pywire.errors` class) | that class's SQLSTATE (table below) | aborts the whole query string |
| `raise UserError(ErrorInfo(severity, code, message, ...))` | exactly that error, every field | aborts the whole query string |
| `return [..., Response.error(ErrorInfo(...))]` | that error for one statement | earlier responses are still sent |
| raise anything else | `XX000` with the exception text | aborts the whole query string |

`UserError(ErrorInfo(...))` works from every callback: simple and extended
query handlers, streaming iterators, COPY handlers, auth sources and session
factories. Use it for codes pywire has no class for, such as
`23505 unique_violation` or `42P01 undefined_table`.

## Exception classes and their SQLSTATEs

pywire mirrors every variant of pgwire's `PgWireError`, under `ProtocolError`
and `AuthError` umbrellas, both rooted at `pywire.errors.Error`. These are the
ones a handler typically raises:

| Class | SQLSTATE | Meaning |
| --- | --- | --- |
| `QueryCanceled` | `57014` | query canceled |
| `InvalidPassword` | `28P01` | password authentication failed |
| `UserNameRequired` | `28000` | missing user name |
| `PortalNotFound`, `StatementNotFound` | `26000` | unknown portal or statement |
| `ParameterIndexOutOfBound`, `InvalidOptionValue` | `22023` | invalid parameter value |
| `FailedToParseParameter` | `22P02` | invalid text representation |
| `UnsupportedCertificateSignatureAlgorithm` | `0A000` | feature not supported |
| `ApiError`, other non-protocol failures | `XX000` | internal error |
| `ProtocolError` subclasses | `08P01` | protocol violation |

The `pywire.messages` codecs raise the same classes for malformed frames
(for example `TruncatedMessage` or `InvalidMessageType`). pgwire's `IoError`
maps to the built-in `OSError`.

## `ErrorInfo`

`ErrorInfo(severity, code, message, *, detail=None, hint=None, position=None,
...)` mirrors PostgreSQL's
[error fields](https://www.postgresql.org/docs/current/protocol-error-fields.html)
one-to-one. With `severity="FATAL"` (`info.is_fatal()`), the server closes the
connection after sending the error.
