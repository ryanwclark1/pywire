"""Runtime facts the stubs promise but stubtest cannot see.

stubtest compares names and parameter kinds; these tests pin the native
defaults that PyO3 renders through `__text_signature__`, and that the
frozen value classes are hashable.
"""

from __future__ import annotations

import inspect

import pytest

from pywire import auth, errors, messages, query


def test_native_defaults_are_introspectable() -> None:
    for method in (query.Response.query, query.Response.stream):
        assert inspect.signature(method).parameters["command_tag"].default == "SELECT"
    login = inspect.signature(auth.LoginInfo).parameters
    assert login["host"].default == "127.0.0.1"
    assert login["client_certificates"].default == ()
    field = inspect.signature(query.FieldInfo).parameters
    assert (field["type_id"].default, field["format"].default) == (25, 0)
    assert inspect.signature(errors.ErrorInfo).parameters["line"].default is None


@pytest.mark.parametrize(
    "value",
    [
        query.FieldInfo("n"),
        auth.LoginInfo(user="alice", client_certificates=[b"der"]),
        auth.Password(b"secret", salt=b"salt"),
        messages.TransactionStatus.Idle,
        messages.Startup(parameters={"user": "alice"}),
        messages.Query("SELECT 1"),
        messages.Terminate(),
        messages.ReadyForQuery(messages.TransactionStatus.Error),
        messages.CommandComplete("SELECT 1"),
        messages.FieldDescription("n", type_id=23),
        messages.RowDescription([messages.FieldDescription("n")]),
        messages.DataRow(1, b"\x00\x00\x00\x011"),
        messages.ErrorResponse([(ord("M"), "boom")]),
    ],
)
def test_value_classes_hash_consistently_with_eq(value: object) -> None:
    assert hash(value) == hash(value)
    assert {value: 1}[value] == 1


def test_login_info_client_certificates_are_bytes() -> None:
    login = auth.LoginInfo(client_certificates=[b"leaf", b"intermediate"])
    assert login.client_certificates == [b"leaf", b"intermediate"]
    assert login == auth.LoginInfo(client_certificates=(b"leaf", b"intermediate"))
