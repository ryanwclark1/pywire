# COPY

`COPY ... FROM STDIN` and `COPY ... TO STDOUT` move bulk data in a
sub-protocol of a normal query. Your handler starts one by returning a COPY
response from `do_query`; pgwire runs the message exchange.

```python
--8<-- "examples/copy_data.py"
```

## COPY FROM STDIN (client → server)

Return `Response.copy_in(handler, columns=n, format=Format.TEXT)`. pgwire
sends `CopyInResponse`, and then:

1. `await handler.on_copy_data(chunk)` runs for each `CopyData` message.
   Chunks are opaque bytes and **need not align with rows**, so buffer them.
2. `await handler.on_copy_done()` runs on `CopyDone`. Return the number of
   rows copied; pywire sends the `COPY n` command tag.
3. If the client aborts, `await handler.on_copy_fail(message)` runs instead,
   and the client gets PostgreSQL's `57014 COPY from stdin failed`, which
   drivers treat as the normal end of an aborted COPY.

The handler object belongs to one COPY on one connection, so per-COPY state
lives on `self`. Raising from `on_copy_data` or `on_copy_done` aborts the COPY
and reports the error.

## COPY TO STDOUT (server → client)

Return `Response.copy_out(chunks, columns=n, format=Format.TEXT)`, where
`chunks` is an async iterable of `bytes`. Each chunk becomes one `CopyData`
message and counts as one row in the `COPY n` tag, so **yield one row per
chunk**.

## Formats

pywire does not parse COPY data. For `format=Format.TEXT` (also used by CSV),
rows are lines in PostgreSQL's text or CSV COPY format. For
`format=Format.BINARY`:

- on COPY FROM, the client's chunks carry the `PGCOPY` header, the tuples and
  the trailer;
- on COPY TO, put the 19-byte header in your **first** chunk, together with
  the first row. pgwire appends the trailer.

!!! warning "Errors inside `copy_out`"
    If the chunk iterator raises, pgwire sends the client a `CopyFail` and
    then the error. That's upstream pgwire behavior; libpq-based clients
    report it as a protocol error rather than the original SQLSTATE.
    Validate the export before you return `copy_out` where you can.
