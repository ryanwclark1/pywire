# Simple queries

A client that sends SQL without parameters uses PostgreSQL's *simple query*
protocol: one `Query` message carrying one or more `;`-separated statements.
pgwire parses the frame and calls your `SimpleQueryHandler.do_query` with the
raw text. You return one `Response` per statement, and pgwire writes the
`RowDescription`, `DataRow`, `CommandComplete` and `ReadyForQuery` messages.

```python
--8<-- "examples/hello.py"
```

Run it with `python examples/hello.py`, then connect with `psql` or any driver.

## Choosing a `Response`

| Constructor | Wire result | Use it for |
| --- | --- | --- |
| `Response.query(fields, rows, command_tag="SELECT")` | `RowDescription`, `DataRow`s, `CommandComplete` | result sets you already hold in memory |
| `Response.stream(fields, rows, command_tag="SELECT")` | the same, pulled lazily | large or slow results ([Streaming](streaming.md)) |
| `Response.execution(command, rows=None, oid=None, transaction=None)` | `CommandComplete` only | `INSERT 0 1`, `UPDATE 3`, `SET`, `BEGIN` |
| `Response.empty()` | `EmptyQueryResponse` | an empty query string (`;`) |
| `Response.error(info)` | `ErrorResponse` | a statement-level failure ([Errors](errors.md)) |
| `Response.copy_in(...)` / `Response.copy_out(...)` | COPY sub-protocol | bulk load/export ([COPY](copy.md)) |

`Response.kind` reports which constructor built a response; its type is the
`pywire.query.ResponseKind` literal.

## Command tags

The tag is what clients show as the status (`cursor.statusmessage` in
psycopg) and parse for row counts:

```python
Response.execution("INSERT", oid=0, rows=1)   # INSERT 0 1
Response.execution("UPDATE", rows=3)          # UPDATE 3
Response.query(fields, rows, command_tag="SHOW")
```

## Transactions

Clients track the transaction state from `ReadyForQuery`. Tell pgwire when a
statement opens or closes a transaction block with `transaction="start"` or
`transaction="end"`. Drivers such as psycopg open one implicitly unless
autocommit is on, so even a toy server should answer `BEGIN`, `COMMIT` and
`ROLLBACK`, as `hello.py` does. Ending a transaction also closes the
connection's open portals, as in PostgreSQL.

## Multiple statements

A simple query may hold several statements; return their responses in order.
PostgreSQL stops at the first failing statement, so a handler mirroring it
appends a `Response.error(...)` and returns.

!!! note "Parsing SQL is your job"
    pywire hands you the query text untouched. Splitting on `;` is fine for
    demos but breaks on string literals; use a real SQL parser (for example
    `sqlglot` or `pglast`) for anything serious.
