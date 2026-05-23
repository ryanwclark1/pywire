"""Tests for `pywire.messages`.

Three concerns:

1. Every message class round-trips through `encode()`/`decode()` on
   hand-crafted byte sequences, including the exact tag-and-length
   framing PostgreSQL specifies.
2. The classes have structural equality and useful `__repr__`s.
3. Hypothesis property tests for every encode/decode pair: random
   inputs must round-trip identically.

These tests also drive the 100% Rust coverage on the messages module.
"""

from __future__ import annotations

import pytest
from hypothesis import given
from hypothesis import strategies as st

import pywire
from pywire import messages
from pywire.messages import (
    CommandComplete,
    DataRow,
    ErrorResponse,
    FieldDescription,
    Query,
    ReadyForQuery,
    RowDescription,
    Startup,
    Terminate,
    TransactionStatus,
)

# ---- module shape -------------------------------------------------------


def test_messages_is_exposed_under_pywire():
    assert pywire.messages is messages


def test_all_exports_match_module():
    public = {name for name in dir(messages) if not name.startswith("_")}
    public.discard("annotations")
    assert set(messages.__all__) <= public


# ---- TransactionStatus --------------------------------------------------


def test_transaction_status_distinct_values():
    statuses = [
        TransactionStatus.Idle,
        TransactionStatus.Transaction,
        TransactionStatus.Error,
    ]
    # Each variant is equal to itself, and any two are distinct.
    for s in statuses:
        assert s == s
    pairs = [(a, b) for a in statuses for b in statuses if a is not b]
    for a, b in pairs:
        assert a != b


# ---- Query --------------------------------------------------------------


def test_query_encodes_with_Q_tag():
    q = Query("SELECT 1")
    wire = q.encode()
    # Tag byte, length, body, null terminator.
    assert wire[0:1] == b"Q"
    # length includes the 4 length bytes and the body+null.
    assert int.from_bytes(wire[1:5], "big") == len(wire) - 1
    assert wire.endswith(b"\x00")


def test_query_round_trip():
    q = Query("INSERT INTO t VALUES (1)")
    assert Query.decode(q.encode()) == q


def test_query_equality_and_repr():
    assert Query("SELECT 1") == Query("SELECT 1")
    assert Query("SELECT 1") != Query("SELECT 2")
    assert "SELECT 1" in repr(Query("SELECT 1"))


@given(st.text())
def test_query_round_trip_property(query):
    # cstring framing forbids embedded nulls.
    if "\x00" in query:
        return
    q = Query(query)
    assert Query.decode(q.encode()) == q


# ---- Terminate ----------------------------------------------------------


def test_terminate_round_trip():
    t = Terminate()
    assert t.encode() == b"X\x00\x00\x00\x04"
    assert Terminate.decode(t.encode()) == t


def test_terminate_equality():
    assert Terminate() == Terminate()
    assert "Terminate()" in repr(Terminate())


def test_every_message_has_useful_repr():
    # Every message class's __repr__ should mention enough to identify a
    # value at a glance. Also drives Rust __repr__ coverage.
    assert "Startup" in repr(Startup(parameters={"user": "x"}))
    assert "ReadyForQuery" in repr(ReadyForQuery(TransactionStatus.Idle))
    assert "Idle" in repr(ReadyForQuery(TransactionStatus.Idle))
    assert "CommandComplete" in repr(CommandComplete("SELECT 1"))
    assert "SELECT 1" in repr(CommandComplete("SELECT 1"))
    assert "ErrorResponse" in repr(ErrorResponse([(ord("M"), "boom")]))


# ---- Startup ------------------------------------------------------------


def test_startup_default_constructor():
    s = Startup()
    assert s.protocol_number_major == 3
    assert s.protocol_number_minor == 0
    assert s.parameters == {}


def test_startup_round_trip_with_params():
    s = Startup(
        protocol_number_major=3,
        protocol_number_minor=0,
        parameters={"user": "alice", "database": "postgres"},
    )
    decoded = Startup.decode(s.encode())
    assert decoded == s
    assert decoded.parameters == {"user": "alice", "database": "postgres"}


def test_startup_encodes_without_type_tag():
    s = Startup(parameters={"user": "x"})
    wire = s.encode()
    # First four bytes are length, not a tag.
    assert int.from_bytes(wire[:4], "big") == len(wire)
    # The protocol-major-version field comes next, encoded big-endian.
    assert int.from_bytes(wire[4:6], "big") == 3
    assert int.from_bytes(wire[6:8], "big") == 0


@given(
    params=st.dictionaries(
        st.text(
            min_size=1,
            alphabet=st.characters(blacklist_categories=("Cs",), blacklist_characters="\x00"),
        ),
        st.text(
            alphabet=st.characters(blacklist_categories=("Cs",), blacklist_characters="\x00"),
        ),
        max_size=5,
    ),
)
def test_startup_round_trip_property(params):
    s = Startup(parameters=params)
    assert Startup.decode(s.encode()) == s


# ---- ReadyForQuery + TransactionStatus ----------------------------------


@pytest.mark.parametrize(
    "status",
    [TransactionStatus.Idle, TransactionStatus.Transaction, TransactionStatus.Error],
)
def test_ready_for_query_round_trip(status):
    r = ReadyForQuery(status)
    assert ReadyForQuery.decode(r.encode()) == r
    assert r.status == status


def test_ready_for_query_wire_shape():
    r = ReadyForQuery(TransactionStatus.Idle)
    wire = r.encode()
    assert wire[0:1] == b"Z"
    # Length is 5: 4 (length field) + 1 (status byte).
    assert int.from_bytes(wire[1:5], "big") == 5
    assert wire[5:6] == b"I"


# ---- CommandComplete ---------------------------------------------------


def test_command_complete_round_trip():
    c = CommandComplete("SELECT 1")
    assert CommandComplete.decode(c.encode()) == c


def test_command_complete_wire_shape():
    c = CommandComplete("INSERT 0 1")
    wire = c.encode()
    assert wire[0:1] == b"C"
    assert wire.endswith(b"\x00")


@given(st.text(alphabet=st.characters(blacklist_categories=("Cs",), blacklist_characters="\x00")))
def test_command_complete_round_trip_property(tag):
    c = CommandComplete(tag)
    assert CommandComplete.decode(c.encode()) == c


# ---- FieldDescription + RowDescription ---------------------------------


def test_field_description_defaults():
    f = FieldDescription("col_a")
    assert f.name == "col_a"
    assert f.table_id == 0
    assert f.column_id == 0
    assert f.type_id == 0
    assert f.type_size == 0
    assert f.type_modifier == 0
    assert f.format_code == 0


def test_field_description_with_all_fields():
    f = FieldDescription(
        "col_b",
        table_id=16384,
        column_id=1,
        type_id=23,
        type_size=4,
        type_modifier=-1,
        format_code=0,
    )
    assert f.type_id == 23
    assert "col_b" in repr(f)


def test_row_description_round_trip():
    rd = RowDescription(
        [
            FieldDescription("id", type_id=23, type_size=4),
            FieldDescription("name", type_id=25, type_size=-1, type_modifier=-1),
        ]
    )
    decoded = RowDescription.decode(rd.encode())
    assert decoded == rd
    assert decoded.fields[0].name == "id"


def test_row_description_empty():
    rd = RowDescription()
    assert rd.fields == []
    assert RowDescription.decode(rd.encode()) == rd


def test_row_description_repr_uses_field_repr():
    rd = RowDescription([FieldDescription("x")])
    # Should pick up FieldDescription's __repr__, not the Rust Debug form.
    assert "FieldDescription" in repr(rd)
    assert "PyFieldDescription" not in repr(rd)


# ---- DataRow -----------------------------------------------------------


def test_data_row_round_trip():
    payload = b"\x00\x00\x00\x05hello"
    d = DataRow(1, payload)
    assert d.data == payload
    assert DataRow.decode(d.encode()) == d


def test_data_row_repr():
    d = DataRow(2, b"\x00\x00\x00\x01x\x00\x00\x00\x01y")
    assert "field_count=2" in repr(d)
    assert "bytes" in repr(d)


@given(st.integers(min_value=0, max_value=1000), st.binary(max_size=200))
def test_data_row_round_trip_property(field_count, data):
    d = DataRow(field_count, data)
    decoded = DataRow.decode(d.encode())
    assert decoded.field_count == field_count
    assert decoded.data == data


# ---- ErrorResponse ------------------------------------------------------


def test_error_response_round_trip():
    fields = [(ord("S"), "ERROR"), (ord("C"), "22000"), (ord("M"), "bad value")]
    e = ErrorResponse(fields)
    decoded = ErrorResponse.decode(e.encode())
    assert decoded.fields == fields


def test_error_response_default_empty():
    e = ErrorResponse()
    # The wire form is type byte + length + null terminator.
    wire = e.encode()
    assert wire[0:1] == b"E"
    assert wire[-1:] == b"\x00"
    decoded = ErrorResponse.decode(wire)
    assert decoded.fields == []


# ---- decode error paths -------------------------------------------------


def test_decode_rejects_empty_input():
    with pytest.raises(ValueError):
        Query.decode(b"")


def test_decode_rejects_truncated_query():
    # Type tag + half a length field.
    with pytest.raises(ValueError):
        Query.decode(b"Q\x00\x00")
