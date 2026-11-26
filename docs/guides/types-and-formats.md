# Types and formats

Every column and parameter on the wire has a PostgreSQL **type OID** and a
**format code**. pywire never converts values: rows and parameters are
`bytes` already encoded the way the client expects. You choose the OIDs and
formats; pgwire writes them verbatim.

## Type OIDs: `pywire.types.Type`

`Type` is an `IntEnum` of every built-in type known to `postgres-types`, the
crate pgwire uses, named like its Rust constants:

```python
from pywire.query import FieldInfo
from pywire.types import Format, Type

FieldInfo("id", type_id=Type.INT8)
FieldInfo("tags", type_id=Type.TEXT_ARRAY)
Type.TEXT_ARRAY.element      # Type.TEXT
Type.JSONB.pg_name           # "jsonb"
```

Members are plain `int`s, so they work anywhere pywire accepts an OID. Types
created with `CREATE TYPE` get per-database OIDs; pass those as integers.
pywire preserves unknown OIDs on the wire.

`pywire.types.type_name(oid)` looks up the PostgreSQL name of any built-in
OID and returns `None` for others.

## Formats: text and binary

| Format | Code | Cell bytes |
| --- | --- | --- |
| `Format.TEXT` | `0` | The value's PostgreSQL text output: `b"42"`, `b"t"`, `b"2026-01-31"`, `b"{1,2,3}"` |
| `Format.BINARY` | `1` | The type's binary send format: `int4` is 4 big-endian bytes, and so on |

`None` is SQL `NULL` in either format.

Set the format per column with `FieldInfo(..., format=Format.BINARY)`. Only
use binary for a column when the client asked for it: in the extended
protocol, the client's requested result formats arrive as
`Portal.result_formats`. The simple query protocol always uses text.

!!! tip "Text values are identical in both formats"
    For `text`, `varchar`, `name` and `bpchar` the binary and text encodings
    are both the UTF-8 bytes, so returning text cells under a binary format
    code is correct. `prepared_statements.py` relies on this.

## Parameters

Extended-protocol parameters arrive as `list[bytes | None]` with one format
code each in `Portal.parameter_formats`. An empty list means "all text", and
a single code applies to every parameter. Decode them according to the
statement's `parameter_types`; psycopg, for example, sends Python `int`s as
binary `int2`, `int4` or `int8` depending on size:

```python
--8<-- "examples/prepared_statements.py:decode_int"
```
