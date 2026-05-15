# Query handlers

`pywire.query` exposes the simple-query handler interface. Subclass
`SimpleQueryHandler` and implement async `do_query` to define how your
pywire server answers `'Q'` (simple-query) requests.

## Quick example

```python
from pywire.query import FieldInfo, Response, SimpleQueryHandler


class HelloHandler(SimpleQueryHandler):
    async def do_query(self, query: str) -> list[Response]:
        if query.strip().lower() == "select 1":
            return [
                Response.query(
                    fields=[FieldInfo("one", type_id=23)],   # int4 OID
                    rows=[[b"1"]],
                ),
            ]
        return [Response.execution(query.split()[0].upper(), rows=0)]
```

## `Response`

A `Response` represents one statement's result within a simple-query
response stream. Construct via the classmethod factories:

| Factory                                                              | Use it when                                                       |
| -------------------------------------------------------------------- | ----------------------------------------------------------------- |
| `Response.empty()`                                                   | The client sent an empty query (just `;`).                        |
| `Response.execution(command, *, oid=None, rows=None)`                | DML / DDL completion (INSERT, UPDATE, DELETE, BEGIN, COMMIT, …).  |
| `Response.query(fields, rows, *, command_tag="SELECT")`              | Rows-returning result (SELECT, RETURNING, …).                     |
| `Response.error(info)`                                               | A statement-level error with structured fields.                   |

The `kind` property returns one of `"empty"`, `"execution"`, `"query"`,
`"error"` and a `repr()` that names the constructor.

### Row payload format

`Response.query` takes `rows: list[list[bytes | None]]`. Each row is a
list of cell payloads, one per column in `fields`. A cell is:

- a `bytes` value — the **text-format** representation
  (e.g. `b"42"` for an int4, `b"alice"` for a text), or
- `None` — SQL NULL.

The encoder writes the wire-level `DataRow` frame for you. Format-code
control (text vs binary) and value-conversion helpers will arrive in
follow-up PRs; for now, encode values to text yourself.

## `FieldInfo`

```python
FieldInfo(name: str, *, type_id: int = 25)
```

`type_id` is the PostgreSQL OID for the column's type. Common OIDs:

| OID | Type    |
| --- | ------- |
| 16  | bool    |
| 20  | int8    |
| 23  | int4    |
| 25  | text (default) |
| 700 | float4  |
| 701 | float8  |
| 1043| varchar |
| 1082| date    |
| 1114| timestamp |
| 1184| timestamptz |

See `pg_type` in any PostgreSQL `psql` session for the full list.

## Errors inside `do_query`

Raise any subclass of [`pywire.errors.Error`](errors.md) to fail the
whole query response. To send a structured statement-level error
(retaining other successful responses), return
`Response.error(ErrorInfo(...))`.

!!! warning "Server bindings not yet shipped"
    The connection state machine that drives `do_query` against a real
    socket ships with `pywire.server` (PR I). Today you can write your
    `SimpleQueryHandler` subclass and have it be fully ready, but you
    can't yet stand up a running server with it.

## Reference

::: pywire.query
    options:
      show_source: false
      heading_level: 3
      members_order: source
