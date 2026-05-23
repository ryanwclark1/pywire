"""Tests for `pywire.auth`.

Three concerns:

1. `LoginInfo` and `Password` construct, expose their fields, and
   `__repr__`/`__eq__` behave.
2. The `AuthSource` ABC refuses to instantiate without `get_password`
   and accepts a concrete subclass.
3. The Rust → Python adapter actually calls a Python coroutine, awaits
   it, and round-trips the resulting `Password` back into Rust. We
   drive this through the hidden `_test_call_get_password` pyfunction.
"""

from __future__ import annotations

import pytest
from pywire._pywire.auth import _test_call_get_password  # type: ignore[import-not-found]

import pywire
from pywire import auth

# ---- module shape -------------------------------------------------------


def test_auth_is_exposed_under_pywire():
    assert pywire.auth is auth


def test_all_exports_match_module():
    public = {name for name in dir(auth) if not name.startswith("_")}
    public.discard("annotations")
    assert set(auth.__all__) <= public


# ---- LoginInfo ---------------------------------------------------------


def test_login_info_defaults_to_local_host():
    info = auth.LoginInfo()
    assert info.user is None
    assert info.database is None
    assert info.host == "127.0.0.1"


def test_login_info_explicit_fields():
    info = auth.LoginInfo(user="alice", database="postgres", host="10.0.0.1")
    assert info.user == "alice"
    assert info.database == "postgres"
    assert info.host == "10.0.0.1"
    assert "alice" in repr(info)
    assert "postgres" in repr(info)
    assert "10.0.0.1" in repr(info)


def test_login_info_equality():
    a = auth.LoginInfo(user="alice", host="h")
    b = auth.LoginInfo(user="alice", host="h")
    assert a == b
    assert a != auth.LoginInfo(user="bob", host="h")


# ---- Password ----------------------------------------------------------


def test_password_cleartext():
    p = auth.Password(b"hunter2")
    assert p.password == b"hunter2"
    assert p.salt is None
    assert "cleartext" in repr(p)


def test_password_with_salt():
    p = auth.Password(b"deadbeef", salt=b"\x01\x02\x03\x04")
    assert p.password == b"deadbeef"
    assert p.salt == b"\x01\x02\x03\x04"
    assert "salted" in repr(p)


def test_password_equality():
    assert auth.Password(b"x") == auth.Password(b"x")
    assert auth.Password(b"x") != auth.Password(b"y")
    assert auth.Password(b"x") != auth.Password(b"x", salt=b"z")


# ---- AuthSource ABC ----------------------------------------------------


def test_auth_source_abstract_cannot_be_instantiated():
    with pytest.raises(TypeError):
        auth.AuthSource()  # type: ignore[abstract]


def test_auth_source_concrete_subclass_works():
    class Static(auth.AuthSource):
        async def get_password(self, login: auth.LoginInfo) -> auth.Password:
            return auth.Password(b"pw")

    src = Static()
    assert isinstance(src, auth.AuthSource)


# ---- Rust adapter (Rust → Python boundary) ----------------------------


async def test_adapter_invokes_python_get_password():
    class Static(auth.AuthSource):
        async def get_password(self, login: auth.LoginInfo) -> auth.Password:
            assert login.user == "alice"
            assert login.database == "postgres"
            assert login.host == "10.0.0.1"
            return auth.Password(b"hunter2")

    pwd = await _test_call_get_password(Static(), "alice", "postgres", "10.0.0.1")
    assert pwd.password == b"hunter2"
    assert pwd.salt is None


async def test_adapter_handles_optional_login_fields():
    class Anon(auth.AuthSource):
        async def get_password(self, login: auth.LoginInfo) -> auth.Password:
            assert login.user is None
            assert login.database is None
            return auth.Password(b"")

    pwd = await _test_call_get_password(Anon(), None, None, "127.0.0.1")
    assert pwd.password == b""


async def test_adapter_propagates_python_exceptions():
    class Refusing(auth.AuthSource):
        async def get_password(self, login: auth.LoginInfo) -> auth.Password:
            from pywire.errors import InvalidPassword

            raise InvalidPassword(login.user or "?")

    with pytest.raises(Exception, match="InvalidPassword|alice"):
        await _test_call_get_password(Refusing(), "alice", None, "h")


async def test_adapter_round_trips_salt():
    class Salted(auth.AuthSource):
        async def get_password(self, login: auth.LoginInfo) -> auth.Password:
            return auth.Password(b"hashed", salt=b"\xaa\xbb")

    pwd = await _test_call_get_password(Salted(), "u", "d", "h")
    assert pwd.password == b"hashed"
    assert pwd.salt == b"\xaa\xbb"
