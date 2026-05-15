"""Tests for `pywire.query`.

Three concerns:

1. `FieldInfo` and `Response` construct via their public surfaces and
   surface a useful `kind` / repr.
2. The `SimpleQueryHandler` ABC refuses direct instantiation and
   accepts concrete subclasses.
3. The Rust adapter calls `do_query` over the runtime bridge and the
   resulting `PyResponse`s convert cleanly to pgwire's `Response`
   enum.
"""

from __future__ import annotations

import pytest
from pywire._pywire.query import _test_drive_handler  # type: ignore[import-not-found]

import pywire
from pywire import errors, query

# ---- module shape -----------------------------------------------------


def test_query_is_exposed_under_pywire():
    assert pywire.query is query


def test_all_exports_match_module():
    public = {name for name in dir(query) if not name.startswith("_")}
    public.discard("annotations")
    assert set(query.__all__) <= public


# ---- FieldInfo --------------------------------------------------------


def test_field_info_defaults_to_text_oid():
    f = query.FieldInfo("name")
    assert f.name == "name"
    assert f.type_id == 25  # TEXT


def test_field_info_with_int4_oid():
    f = query.FieldInfo("id", type_id=23)
    assert f.type_id == 23
    assert "id" in repr(f)
    assert "23" in repr(f)


def test_field_info_equality():
    assert query.FieldInfo("a") == query.FieldInfo("a")
    assert query.FieldInfo("a") != query.FieldInfo("b")


# ---- Response constructors -------------------------------------------


def test_response_empty():
    r = query.Response.empty()
    assert r.kind == "empty"
    assert repr(r) == "Response.empty()"


def test_response_execution_minimal():
    r = query.Response.execution("INSERT")
    assert r.kind == "execution"
    assert "INSERT" in repr(r)


def test_response_execution_with_oid_and_rows():
    r = query.Response.execution("INSERT", oid=0, rows=5)
    assert r.kind == "execution"
    assert "rows" in repr(r)


def test_response_query():
    r = query.Response.query(
        fields=[query.FieldInfo("id", type_id=23)],
        rows=[[b"1"], [b"2"]],
    )
    assert r.kind == "query"
    assert "rows=2" in repr(r)


def test_response_query_with_custom_tag():
    r = query.Response.query(
        fields=[query.FieldInfo("v")],
        rows=[[b"hello"]],
        command_tag="SELECT 1",
    )
    assert "SELECT 1" in repr(r)


def test_response_error():
    info = errors.ErrorInfo("ERROR", "22000", "bad")
    r = query.Response.error(info)
    assert r.kind == "error"
    assert "Response.error" in repr(r)


# ---- SimpleQueryHandler ABC ------------------------------------------


def test_handler_abstract_cannot_be_instantiated():
    with pytest.raises(TypeError):
        query.SimpleQueryHandler()  # type: ignore[abstract]


def test_handler_concrete_subclass_works():
    class Echo(query.SimpleQueryHandler):
        async def do_query(self, q: str) -> list[query.Response]:
            return [query.Response.execution(q)]

    assert isinstance(Echo(), query.SimpleQueryHandler)


# ---- Rust adapter end-to-end -----------------------------------------


async def test_adapter_handles_empty():
    class Empty(query.SimpleQueryHandler):
        async def do_query(self, q: str) -> list[query.Response]:
            return [query.Response.empty()]

    out = await _test_drive_handler(Empty(), ";")
    assert out == [("empty", "")]


async def test_adapter_handles_execution_with_oid_and_rows():
    class Insert(query.SimpleQueryHandler):
        async def do_query(self, q: str) -> list[query.Response]:
            return [query.Response.execution("INSERT", oid=0, rows=42)]

    out = await _test_drive_handler(Insert(), "INSERT ...")
    assert out == [("execution", "INSERT oid=0 rows=42")]


async def test_adapter_handles_query_rows():
    class Select(query.SimpleQueryHandler):
        async def do_query(self, q: str) -> list[query.Response]:
            return [
                query.Response.query(
                    fields=[
                        query.FieldInfo("id", type_id=23),
                        query.FieldInfo("name", type_id=25),
                    ],
                    rows=[
                        [b"1", b"alice"],
                        [b"2", None],
                    ],
                ),
            ]

    out = await _test_drive_handler(Select(), "SELECT * FROM t")
    assert len(out) == 1
    kind, summary = out[0]
    assert kind == "query"
    assert "tag=SELECT" in summary
    assert "fields=2" in summary
    assert "rows=2" in summary


async def test_adapter_handles_error_response():
    class Failing(query.SimpleQueryHandler):
        async def do_query(self, q: str) -> list[query.Response]:
            info = errors.ErrorInfo("ERROR", "22000", "data exception")
            return [query.Response.error(info)]

    out = await _test_drive_handler(Failing(), "SELECT 1")
    kind, summary = out[0]
    assert kind == "error"
    assert summary == "ERROR/22000/data exception"


async def test_adapter_returns_multiple_responses():
    """A simple-query string can contain multiple statements."""

    class Multi(query.SimpleQueryHandler):
        async def do_query(self, q: str) -> list[query.Response]:
            return [
                query.Response.execution("BEGIN"),
                query.Response.execution("INSERT", rows=1),
                query.Response.execution("COMMIT"),
            ]

    out = await _test_drive_handler(Multi(), "BEGIN; INSERT ...; COMMIT;")
    assert [k for k, _ in out] == ["execution", "execution", "execution"]


async def test_adapter_propagates_python_exception():
    class Boom(query.SimpleQueryHandler):
        async def do_query(self, q: str) -> list[query.Response]:
            raise errors.QueryCanceled("user cancel")

    with pytest.raises(Exception, match="cancel"):
        await _test_drive_handler(Boom(), "SELECT 1")


# ---- Extended query shapes -------------------------------------------


def test_prepared_statement_dataclass():
    stmt = query.PreparedStatement(
        name="s1",
        query="SELECT $1::int",
        parameter_types=[23],
    )
    assert stmt.name == "s1"
    assert stmt.parameter_types == [23]
    # frozen
    with pytest.raises(AttributeError):
        stmt.name = "s2"  # type: ignore[misc]


def test_portal_dataclass():
    stmt = query.PreparedStatement(name="", query="SELECT 1", parameter_types=[])
    portal = query.Portal(
        name="p1",
        statement=stmt,
        parameters=[b"42"],
        result_formats=[0],
    )
    assert portal.statement is stmt
    assert portal.parameters == [b"42"]


def test_describe_statement_response():
    r = query.DescribeStatementResponse(
        parameter_types=[23],
        fields=[query.FieldInfo("v", type_id=25)],
    )
    assert r.parameter_types == [23]
    assert r.fields[0].name == "v"


def test_describe_portal_response():
    r = query.DescribePortalResponse(fields=[query.FieldInfo("x")])
    assert len(r.fields) == 1


def test_extended_query_handler_is_abstract():
    with pytest.raises(TypeError):
        query.ExtendedQueryHandler()  # type: ignore[abstract]


async def test_extended_query_handler_concrete_subclass():
    class NoopExtended(query.ExtendedQueryHandler):
        async def parse_statement(
            self,
            name: str,
            q: str,
            parameter_types: list[int],
        ) -> query.PreparedStatement:
            return query.PreparedStatement(name=name, query=q, parameter_types=parameter_types)

        async def describe_statement(
            self,
            statement: query.PreparedStatement,
        ) -> query.DescribeStatementResponse:
            return query.DescribeStatementResponse(
                parameter_types=statement.parameter_types,
                fields=[],
            )

        async def bind_portal(
            self,
            name: str,
            statement: query.PreparedStatement,
            parameters: list[bytes | None],
            result_formats: list[int],
        ) -> query.Portal:
            return query.Portal(
                name=name,
                statement=statement,
                parameters=parameters,
                result_formats=result_formats,
            )

        async def describe_portal(self, portal: query.Portal) -> query.DescribePortalResponse:
            return query.DescribePortalResponse(fields=[])

        async def do_query(self, portal: query.Portal, max_rows: int) -> query.Response:
            return query.Response.execution("SELECT", rows=0)

    h = NoopExtended()
    stmt = await h.parse_statement("s1", "SELECT 1", [])
    assert stmt.name == "s1"
    portal = await h.bind_portal("p1", stmt, [], [])
    assert portal.statement is stmt
    resp = await h.do_query(portal, 0)
    assert resp.kind == "execution"
    # default no-ops:
    await h.close_statement("s1")
    await h.close_portal("p1")
