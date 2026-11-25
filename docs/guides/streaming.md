# Streaming results

`Response.query` needs every row in memory before anything is sent.
`Response.stream` takes an **async iterable** of rows instead. pgwire pulls
the next row only when it is ready to write it, so a slow client applies
backpressure to your generator and memory stays flat.

```python
--8<-- "examples/streaming.py"
```

## Details

- Any async iterable works: an async generator, or an object with
  `__aiter__`. Passing something else raises `TypeError` immediately.
- Each item is one row, a `list[bytes | None]` in the column formats you
  declared.
- Raising from the iterator ends the result with an `ErrorResponse`. Raise a
  `pywire.errors.Error` subclass (or `UserError(ErrorInfo(...))`) to choose
  the SQLSTATE.
- If the client cancels the query or disconnects, the iterator stops being
  pulled. Release resources in a `finally` block or an
  `async with` inside the generator.
- Streaming works the same way from `do_query` in the extended protocol,
  where `max_rows` lets the client page through the stream.
