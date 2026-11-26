"""`pywire.types`: OID and format constants stay in step with upstream."""

from __future__ import annotations

import asyncio

import psycopg
import pytest

import pywire
from pywire import query
from pywire._pywire.types import builtin_types
from pywire.types import Format, Type, type_name
from tests.test_server import _running_server


def test_type_enum_matches_postgres_types_exactly() -> None:
    """Regenerate with `scripts/gen_types.py` when this fails after an upgrade."""
    expected = {
        (f"{name[1:].upper()}_ARRAY" if name.startswith("_") else name.upper()): oid
        for name, oid in builtin_types()
    }
    assert {member.name: member.value for member in Type} == expected


def test_members_are_plain_oids() -> None:
    assert [int(Type.TEXT), int(Type.INT4), int(Type.JSONB)] == [25, 23, 3802]
    assert [int(Format.TEXT), int(Format.BINARY)] == [0, 1]
    assert isinstance(Type.TEXT, int)
    assert pywire.Type is Type and pywire.Format is Format


def test_names_and_array_elements() -> None:
    assert Type.INT4.pg_name == "int4"
    assert Type.TEXT_ARRAY.pg_name == "_text"
    assert Type.INT4_ARRAY.element is Type.INT4
    assert Type.INT4.element is None
    assert type_name(Type.UUID) == "uuid"
    assert type_name(999_999) is None


@pytest.mark.parametrize(("oid", "value"), [(Type.INT4, 42), (Type.TEXT, "hi"), (Type.BOOL, True)])
async def test_type_constants_reach_the_client(oid: Type, value: object) -> None:
    text = {Type.INT4: b"42", Type.TEXT: b"hi", Type.BOOL: b"t"}[oid]

    class Typed(query.SimpleQueryHandler):
        async def do_query(self, q: str) -> list[query.Response]:
            field = query.FieldInfo("v", type_id=oid, format=Format.TEXT)
            return [query.Response.query([field], [[text]])]

    async with _running_server(Typed()) as port:

        def fetch() -> object:
            with psycopg.connect(f"host=127.0.0.1 port={port} sslmode=disable") as connection:
                row = connection.execute("SELECT v").fetchone()
                assert row is not None
                return row[0]

        assert await asyncio.to_thread(fetch) == value
