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
                    fields=[FieldInfo("one", type_id=23)],  # int4 OID
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
| `Response.stream(fields, rows, *, command_tag="SELECT")`             | Async row stream with wire-level backpressure.                    |
| `Response.error(info)`                                               | A statement-level error with structured fields.                   |

The `kind` property returns one of `"empty"`, `"execution"`, `"query"`,
`"stream"`, `"error"` and a `repr()` that names the constructor.

### Row payload format

`Response.query` takes `rows: list[list[bytes | None]]`. Each row is a
list of cell payloads, one per column in `fields`. A cell is:

- a `bytes` value in the text or binary format declared by
  `FieldInfo.format`, or
- `None` — SQL NULL.

The encoder writes the wire-level `DataRow` frame for you. Use format `0`
for text and `1` for binary. `Response.stream` accepts an async iterable and
pulls one row at a time as the socket becomes writable.

## `FieldInfo`

```python
FieldInfo(name: str, *, type_id: int = 25, format: int = 0)
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

## Reference

::: pywire.query
    options:
      show_source: false
      heading_level: 3
      members_order: source
