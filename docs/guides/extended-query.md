# Prepared statements

Drivers use the *extended query* protocol whenever a query has parameters,
and for explicit prepared statements. Instead of one `Query` message the
client sends a pipeline:

```
Parse ─▶ Bind ─▶ Describe ─▶ Execute ─▶ Sync
  │        │         │           │
parse_  bind_    describe_    do_query(portal, max_rows)
statement portal  portal / _statement
```

pgwire tracks statement and portal state per connection and calls your
`ExtendedQueryHandler` for each step. Pass it as `serve(..., extended=...)`.
Without one, extended-protocol clients get an error; there is no automatic
fallback to the simple handler.

```python
--8<-- "examples/prepared_statements.py"
```

## The callbacks

| Method | Called on | Returns |
| --- | --- | --- |
| `parse_statement(name, query, parameter_types)` | `Parse` | a `PreparedStatement` (keep whatever you parsed on it) |
| `describe_statement(statement)` | `Describe 'S'` | `DescribeStatementResponse(parameter_types, fields)` |
| `bind_portal(name, statement, parameters, parameter_formats, result_formats)` | `Bind` | a `Portal` |
| `describe_portal(portal)` | `Describe 'P'` | `DescribePortalResponse(fields)` |
| `do_query(portal, max_rows)` | `Execute` | a `Response` |
| `close_statement(name)` / `close_portal(name)` | `Close`, and cleanup | nothing (optional) |

Unnamed statements and portals, the common case for one-shot parameterized
queries, use pgwire's `POSTGRESQL_DEFAULT_NAME` marker as their name.

### Parameter types

`parameter_types` holds the OIDs the client declared, with `0` for "infer
it". Return the types you resolved from `parse_statement` and
`describe_statement`, because clients use them to encode parameters.

### Returning rows

`do_query` returns one `Response`, usually `Response.query` or
`Response.stream`, for the portal. When `max_rows` is non-zero, pgwire sends
at most that many rows and suspends the portal; the next `Execute` resumes
it. The Python portal object you returned from `bind_portal` is reused for
every `Describe` and `Execute` on that portal.

### Resource cleanup

Every statement and portal you hand back is eventually released: on `Close`,
on transaction end (portals), when the unnamed statement is replaced, and on
disconnect. Each release calls `close_statement` or `close_portal`, so
backends with server-side resources (database cursors, open files) can free
them.

## One object for both protocols

`serve` takes separate simple and extended handlers, which keeps their
`do_query` signatures apart. A [session](sessions.md) object, though, answers
both, so it implements one `do_query` that accepts either call. See
`pywire.session` for the `typing.overload` pattern.
