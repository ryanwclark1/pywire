"""Static typing probe for the public API (checked by mypy and pyright, never run).

Each `assert_type` pins the type a user's checker sees, so a regression to
`Any`/`Unknown` or a drifting signature fails `make typecheck`.
"""

from __future__ import annotations

from collections.abc import AsyncIterator
from typing import Literal, assert_type

import pywire
from pywire import auth, errors, messages, query, server
from pywire.types import Format, Type


async def _rows() -> AsyncIterator[list[bytes | None]]:
    yield [b"1"]


def check_query() -> None:
    field = query.FieldInfo("n", type_id=23)
    assert_type(field.name, str)
    assert_type(field.type_id, int)
    assert_type(hash(field), int)
    response = query.Response.query([field], [[b"1", None]])
    assert_type(response, query.Response)
    assert_type(response.kind, query.ResponseKind)
    assert_type(query.Response.stream([field], _rows()), query.Response)
    assert_type(query.Response.execution("BEGIN", transaction="start"), query.Response)
    info = errors.ErrorInfo("ERROR", "42601", "syntax error", line=3)
    assert_type(info.line, int | None)
    assert_type(query.Response.error(info), query.Response)
    statement = query.PreparedStatement("s", "SELECT $1", [23])
    assert_type(statement.parameter_types, list[int])
    portal = query.Portal("", statement)
    assert_type(portal.parameters, list[bytes | None])


def check_auth() -> None:
    login = auth.LoginInfo(user="alice", sni_server_name="db.example")
    assert_type(login.user, str | None)
    assert_type(login.client_certificates, list[bytes])
    password = auth.Password(b"secret")
    assert_type(password.salt, bytes | None)
    verifier = auth.ScramVerifier.parse("SCRAM-SHA-256$4096:c2FsdA==$a2V5:a2V5")
    assert_type(verifier.iterations, int)


def check_messages() -> None:
    frame = messages.Query("SELECT 1").encode()
    assert_type(frame, bytes)
    assert_type(messages.Query.decode(frame), messages.Query)
    ready = messages.ReadyForQuery(messages.TransactionStatus.Idle)
    assert_type(ready.status, messages.TransactionStatus)
    assert_type(messages.ErrorResponse([(ord("M"), "boom")]).fields, list[tuple[int, str]])


def check_server() -> None:
    tls = server.TLSConfig("server.crt", "server.key", client_ca="ca.crt")
    assert_type(tls.client_ca, str | None)
    assert_type(pywire.supported_protocol_range(), tuple[int, int])


def check_types() -> None:
    assert_type(Type.INT4_ARRAY.element, Type | None)
    assert_type(Type.JSONB.pg_name, str)
    query.FieldInfo("id", type_id=Type.INT8, format=Format.BINARY)


def check_errors() -> None:
    try:
        raise errors.TruncatedMessage("short frame")
    except errors.ProtocolError as error:
        assert_type(error, errors.ProtocolError)


_kinds: list[Literal["empty", "execution", "query", "stream", "error"]] = ["query"]
_also: list[query.ResponseKind] = _kinds
