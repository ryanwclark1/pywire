# Quickstart

This page builds a PostgreSQL-compatible server in about twenty lines and
queries it with `psql`. It assumes pywire is [installed](install.md).

## 1. Write a handler

A handler receives SQL text and returns `Response` objects. pgwire turns
them into PostgreSQL wire messages.

```python
--8<-- "examples/hello.py"
```

## 2. Run it

```bash
python examples/hello.py          # listens on 127.0.0.1:5433
```

Set `PYWIRE_ADDR=0.0.0.0:6543` to listen elsewhere.

## 3. Connect

Any PostgreSQL client works:

```console
$ psql "host=127.0.0.1 port=5433 sslmode=disable" -c "SELECT 1"
   greeting
--------------
 hello, world
(1 row)
```

or from Python:

```python
import psycopg

with psycopg.connect("host=127.0.0.1 port=5433 sslmode=disable") as conn:
    print(conn.execute("SELECT 1").fetchone())   # ('hello, world',)
```

## Where next

- Return real data with the right [types and formats](guides/types-and-formats.md).
- Support drivers' parameterized queries with
  [prepared statements](guides/extended-query.md).
- Add [authentication](guides/authentication.md) and [TLS](guides/tls.md)
  before exposing a server.
- Keep state per client with [sessions](guides/sessions.md).
- Load and export data with [COPY](guides/copy.md).

Every example in the user guide is in the repository's `examples/` directory
and is run by the test suite.
