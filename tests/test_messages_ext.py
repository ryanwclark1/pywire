"""Extended-query, COPY, startup/auth and notice codecs in `pywire.messages`.

Every codec must round-trip through pgwire (`decode(encode(m)) == m`),
produce the documented type tag, hash like it compares, and have a repr.
"""

from __future__ import annotations

import struct
from typing import Protocol

import pytest
from hypothesis import given
from hypothesis import strategies as st

from pywire import errors, messages

# Text that survives PostgreSQL's NUL-terminated strings.
cstring = st.text(alphabet=st.characters(exclude_categories=["Cs"], exclude_characters="\0"))
oids = st.lists(st.integers(min_value=0, max_value=2**32 - 1), max_size=8)
formats = st.lists(st.sampled_from([0, 1]), max_size=8)
values = st.lists(st.one_of(st.none(), st.binary(max_size=32)), max_size=8)


class Codec(Protocol):
    def encode(self) -> bytes: ...


SAMPLES: list[tuple[Codec, bytes]] = [
    (messages.Parse("SELECT $1", name="s1", type_oids=[23]), b"P"),
    (messages.ParseComplete(), b"1"),
    (messages.Bind(portal_name="p", statement_name="s1", parameters=[b"1", None]), b"B"),
    (messages.BindComplete(), b"2"),
    (messages.Describe("S", "s1"), b"D"),
    (messages.Execute("p", 10), b"E"),
    (messages.Close("P", "p"), b"C"),
    (messages.CloseComplete(), b"3"),
    (messages.Sync(), b"S"),
    (messages.Flush(), b"H"),
    (messages.ParameterDescription([23, 25]), b"t"),
    (messages.NoData(), b"n"),
    (messages.PortalSuspended(), b"s"),
    (messages.EmptyQueryResponse(), b"I"),
    (messages.CopyData(b"1\ta\n"), b"d"),
    (messages.CopyDone(), b"c"),
    (messages.CopyFail("oops"), b"f"),
    (messages.CopyInResponse(0, 2), b"G"),
    (messages.CopyOutResponse(1, 1, [1]), b"H"),
    (messages.Authentication("ok"), b"R"),
    (messages.Authentication("md5_password", salt=b"salt"), b"R"),
    (messages.Authentication("sasl", mechanisms=["SCRAM-SHA-256"]), b"R"),
    (messages.Authentication("sasl_continue", data=b"r=abc"), b"R"),
    (messages.PasswordMessage("hunter2"), b"p"),
    (messages.SASLInitialResponse("SCRAM-SHA-256", b"n,,n=,r=abc"), b"p"),
    (messages.SASLResponse(b"c=biws"), b"p"),
    (messages.ParameterStatus("server_version", "17.0"), b"S"),
    (messages.BackendKeyData(42, b"\x00\x00\x01\x02"), b"K"),
    (messages.NoticeResponse([(ord("S"), "NOTICE"), (ord("M"), "hello")]), b"N"),
    (messages.NotificationResponse(7, "jobs", "ready"), b"A"),
]


@pytest.mark.parametrize(("message", "tag"), SAMPLES, ids=lambda value: repr(value)[:40])
def test_round_trip_tag_hash_and_repr(message: Codec, tag: bytes) -> None:
    frame = message.encode()
    assert frame[:1] == tag
    decoded = type(message).decode(frame)  # type: ignore[attr-defined]
    assert decoded == message
    assert hash(decoded) == hash(message)
    assert repr(message).startswith(type(message).__name__ + "(")


def test_untagged_frontend_messages() -> None:
    ssl = messages.SslRequest().encode()
    assert ssl == struct.pack("!ii", 8, 80877103)
    assert messages.SslRequest.decode(ssl) == messages.SslRequest()
    cancel = messages.CancelRequest(9, b"\x00\x00\x00\x2a")
    frame = cancel.encode()
    assert frame == struct.pack("!iiii", 16, 80877102, 9, 42)
    assert messages.CancelRequest.decode(frame) == cancel
    assert cancel.secret_key == b"\x00\x00\x00\x2a"
    assert "secret" in repr(cancel) and "42" not in repr(cancel)


def test_long_secret_keys_encode_as_bytes() -> None:
    key = bytes(range(32))
    frame = messages.BackendKeyData(1, key).encode()
    assert frame.endswith(key)


def test_unnamed_statements_and_portals_are_empty_strings() -> None:
    parse = messages.Parse.decode(messages.Parse("SELECT 1").encode())
    assert (parse.name, parse.type_oids) == ("", [])
    bind = messages.Bind.decode(messages.Bind().encode())
    assert (bind.portal_name, bind.statement_name, bind.parameters) == ("", "", [])
    assert messages.Execute.decode(messages.Execute().encode()).name == ""
    assert messages.Describe.decode(messages.Describe("P").encode()).name == ""


def test_field_accessors() -> None:
    bind = messages.Bind(parameters=[b"x", None], parameter_formats=[1], result_formats=[0])
    assert bind.parameters == [b"x", None]
    assert (bind.parameter_formats, bind.result_formats) == ([1], [0])
    assert messages.CopyInResponse(1, 3).column_formats == [1, 1, 1]
    assert messages.CopyData(b"abc").data == b"abc"
    assert messages.CopyFail("why").message == "why"
    assert messages.SASLResponse(b"x").data == b"x"
    assert messages.SASLInitialResponse("PLAIN").data is None
    assert messages.PasswordMessage("pw").password == "pw"
    assert "pw" not in repr(messages.PasswordMessage("pw"))
    status = messages.ParameterStatus("a", "b")
    assert (status.name, status.value) == ("a", "b")
    note = messages.NotificationResponse(1, "c")
    assert (note.pid, note.channel, note.payload) == (1, "c", "")
    assert messages.NoticeResponse().fields == []
    assert messages.ParameterDescription().types == []


@pytest.mark.parametrize(
    ("kind", "kwargs", "salt", "mechanisms", "data"),
    [
        ("ok", {}, None, [], None),
        ("cleartext_password", {}, None, [], None),
        ("kerberos_v5", {}, None, [], None),
        ("md5_password", {"salt": b"abcd"}, b"abcd", [], None),
        ("sasl", {"mechanisms": ["SCRAM-SHA-256-PLUS"]}, None, ["SCRAM-SHA-256-PLUS"], None),
        ("sasl_continue", {"data": b"r=1"}, None, [], b"r=1"),
        ("sasl_final", {"data": b"v=2"}, None, [], b"v=2"),
    ],
)
def test_authentication_kinds(
    kind: messages.AuthenticationKind,
    kwargs: dict[str, object],
    salt: bytes | None,
    mechanisms: list[str],
    data: bytes | None,
) -> None:
    auth = messages.Authentication(kind, **kwargs)  # type: ignore[arg-type]
    decoded = messages.Authentication.decode(auth.encode())
    assert (decoded.kind, decoded.salt, decoded.mechanisms, decoded.data) == (
        kind,
        salt,
        mechanisms,
        data,
    )


@pytest.mark.parametrize(
    ("build", "match"),
    [
        (lambda: messages.Authentication("md5_password"), "4-byte salt"),
        (lambda: messages.Authentication("md5_password", salt=b"xy"), "4-byte salt"),
        (lambda: messages.Authentication("sasl_final"), "requires data"),
        (lambda: messages.Authentication("gss"), "unknown Authentication kind"),  # type: ignore[arg-type]
        (lambda: messages.Describe("X"), "target must be"),  # type: ignore[arg-type]
        (lambda: messages.Close(""), "target must be"),  # type: ignore[arg-type]
    ],
)
def test_invalid_construction(build: object, match: str) -> None:
    with pytest.raises(ValueError, match=match):
        build()  # type: ignore[operator]


def test_decode_rejects_wrong_or_short_frames() -> None:
    with pytest.raises(ValueError, match="incomplete"):
        messages.Parse.decode(messages.Parse("SELECT 1").encode()[:-1])
    with pytest.raises(errors.ProtocolError):
        messages.Bind.decode(messages.Sync().encode())


@given(name=cstring, query=cstring, type_oids=oids)
def test_parse_round_trip_property(name: str, query: str, type_oids: list[int]) -> None:
    message = messages.Parse(query, name=name, type_oids=type_oids)
    assert messages.Parse.decode(message.encode()) == message


@given(
    portal=cstring,
    statement=cstring,
    parameters=values,
    parameter_formats=formats,
    result_formats=formats,
)
def test_bind_round_trip_property(
    portal: str,
    statement: str,
    parameters: list[bytes | None],
    parameter_formats: list[int],
    result_formats: list[int],
) -> None:
    message = messages.Bind(
        portal_name=portal,
        statement_name=statement,
        parameters=parameters,
        parameter_formats=parameter_formats,
        result_formats=result_formats,
    )
    assert messages.Bind.decode(message.encode()) == message


@given(st.binary(max_size=256))
def test_copy_data_round_trip_property(data: bytes) -> None:
    assert messages.CopyData.decode(messages.CopyData(data).encode()).data == data
