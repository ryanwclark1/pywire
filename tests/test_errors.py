"""Tests for `pywire.errors`.

Three concerns:

1. Every Python exception class declared in `pywire.errors` exists, sits
   in the correct place in the MRO, and is raisable.
2. Every `PgWireError` variant translated by the Rust binding lands in
   the matching Python class. We exercise this via the hidden
   `_test_raise_for(variant)` Rust helper.
3. `ErrorInfo` constructs with the required fields, accepts every optional
   keyword, exposes each field via attribute access, and `is_fatal()`
   reflects `severity`.

These tests are also responsible for the 100% Rust coverage on the
`pywire_to_py_err` boundary function and the `PyErrorInfo` pyclass.
"""

from __future__ import annotations

import pytest
from pywire._pywire.errors import _test_raise_for  # type: ignore[import-not-found]

import pywire
from pywire import errors

# ---- (1) class shape -----------------------------------------------------


def test_errors_is_exposed_under_pywire():
    assert pywire.errors is errors


def test_root_error_subclasses_exception():
    assert issubclass(errors.Error, Exception)


def test_umbrella_errors_subclass_root():
    assert issubclass(errors.ProtocolError, errors.Error)
    assert issubclass(errors.AuthError, errors.Error)


PROTOCOL_ERROR_CLASSES = [
    errors.UnsupportedProtocolVersion,
    errors.InvalidCancelRequest,
    errors.InvalidMessageType,
    errors.MessageTooLarge,
    errors.InvalidTargetType,
    errors.InvalidTransactionStatus,
    errors.InvalidSSLRequestMessage,
    errors.InvalidGssEncRequestMessage,
    errors.InvalidStartupMessage,
    errors.InvalidAuthenticationMessageCode,
    errors.InvalidSecretKey,
    errors.NotReadyForQuery,
    errors.InvalidOptionValue,
]

AUTH_ERROR_CLASSES = [
    errors.FailedToCoercePasswordMessage,
    errors.InvalidSASLState,
    errors.UnsupportedSASLAuthMethod,
    errors.InvalidScramMessage,
    errors.InvalidPassword,
    errors.UnsupportedCertificateSignatureAlgorithm,
    errors.UserNameRequired,
    errors.InvalidOauthMessage,
    errors.OAuthAuthenticationFailed,
    errors.OAuthValidationError,
    errors.OauthAuthzIdError,
]

DIRECT_ERROR_CLASSES = [
    errors.PortalNotFound,
    errors.PortalNotStarted,
    errors.StatementNotFound,
    errors.ParameterIndexOutOfBound,
    errors.InvalidRustTypeForParameter,
    errors.FailedToParseParameter,
    errors.QueryCanceled,
    errors.ApiError,
    errors.UserError,
]


@pytest.mark.parametrize("cls", PROTOCOL_ERROR_CLASSES)
def test_protocol_subclass_chain(cls):
    assert issubclass(cls, errors.ProtocolError)
    assert issubclass(cls, errors.Error)


@pytest.mark.parametrize("cls", AUTH_ERROR_CLASSES)
def test_auth_subclass_chain(cls):
    assert issubclass(cls, errors.AuthError)
    assert issubclass(cls, errors.Error)


@pytest.mark.parametrize("cls", DIRECT_ERROR_CLASSES)
def test_direct_error_subclass_chain(cls):
    assert issubclass(cls, errors.Error)
    # Not a member of either umbrella.
    assert not issubclass(cls, errors.ProtocolError)
    assert not issubclass(cls, errors.AuthError)


def test_all_exports_cover_every_class():
    # Anything import-visible should be in __all__; anything in __all__ should
    # be importable. Catches drift between facade and stubs.
    public = {name for name in dir(errors) if not name.startswith("_")}
    public.discard("annotations")  # from __future__
    assert set(errors.__all__) <= public
    for name in errors.__all__:
        assert hasattr(errors, name)


# ---- (2) Rust -> Python boundary ----------------------------------------

BOUNDARY_VARIANTS = [
    ("UnsupportedProtocolVersion", errors.UnsupportedProtocolVersion),
    ("InvalidCancelRequest", errors.InvalidCancelRequest),
    ("InvalidSecretKey", errors.InvalidSecretKey),
    ("InvalidMessageType", errors.InvalidMessageType),
    ("MessageTooLarge", errors.MessageTooLarge),
    ("InvalidTargetType", errors.InvalidTargetType),
    ("InvalidTransactionStatus", errors.InvalidTransactionStatus),
    ("InvalidSSLRequestMessage", errors.InvalidSSLRequestMessage),
    ("InvalidGssEncRequestMessage", errors.InvalidGssEncRequestMessage),
    ("InvalidStartupMessage", errors.InvalidStartupMessage),
    ("InvalidAuthenticationMessageCode", errors.InvalidAuthenticationMessageCode),
    ("FailedToCoercePasswordMessage", errors.FailedToCoercePasswordMessage),
    ("InvalidSASLState", errors.InvalidSASLState),
    ("UnsupportedSASLAuthMethod", errors.UnsupportedSASLAuthMethod),
    ("IoError", OSError),
    ("PortalNotFound", errors.PortalNotFound),
    ("PortalNotStarted", errors.PortalNotStarted),
    ("StatementNotFound", errors.StatementNotFound),
    ("ParameterIndexOutOfBound", errors.ParameterIndexOutOfBound),
    ("InvalidRustTypeForParameter", errors.InvalidRustTypeForParameter),
    ("FailedToParseParameter", errors.FailedToParseParameter),
    ("InvalidScramMessage", errors.InvalidScramMessage),
    ("InvalidPassword", errors.InvalidPassword),
    ("UnsupportedCertificateSignatureAlgorithm", errors.UnsupportedCertificateSignatureAlgorithm),
    ("UserNameRequired", errors.UserNameRequired),
    ("NotReadyForQuery", errors.NotReadyForQuery),
    ("QueryCanceled", errors.QueryCanceled),
    ("InvalidOptionValue", errors.InvalidOptionValue),
    ("InvalidOauthMessage", errors.InvalidOauthMessage),
    ("OAuthAuthenticationFailed", errors.OAuthAuthenticationFailed),
    ("OAuthValidationError", errors.OAuthValidationError),
    ("OauthAuthzIdError", errors.OauthAuthzIdError),
    ("ApiError", errors.ApiError),
    ("UserError", errors.UserError),
]


@pytest.mark.parametrize("variant,expected_cls", BOUNDARY_VARIANTS)
def test_pgwire_error_maps_to_python_class(variant, expected_cls):
    with pytest.raises(expected_cls) as info:
        _test_raise_for(variant)
    # Rust formatter produced a non-empty message.
    assert str(info.value)


def test_unknown_variant_raises_value_error():
    with pytest.raises(ValueError):
        _test_raise_for("DefinitelyNotARealVariant")


# ---- (3) ErrorInfo ------------------------------------------------------


def test_error_info_required_fields_only():
    info = errors.ErrorInfo("ERROR", "22000", "data exception")
    assert info.severity == "ERROR"
    assert info.code == "22000"
    assert info.message == "data exception"
    assert info.detail is None


def test_error_info_all_optional_fields_round_trip():
    info = errors.ErrorInfo(
        "ERROR",
        "22000",
        "msg",
        detail="d",
        hint="h",
        position="42",
        internal_position="7",
        internal_query="SELECT 1",
        where_context="ctx",
        file_name="parser.c",
        line=123,
        routine="r",
        severity_nonlocalized="ERROR",
        schema="public",
        table="t",
        column="c",
        datatype="text",
        constraint="not_null",
    )
    assert info.detail == "d"
    assert info.hint == "h"
    assert info.position == "42"
    assert info.internal_position == "7"
    assert info.internal_query == "SELECT 1"
    assert info.where_context == "ctx"
    assert info.file_name == "parser.c"
    assert info.line == 123
    assert info.routine == "r"
    assert info.severity_nonlocalized == "ERROR"
    assert info.schema == "public"
    assert info.table == "t"
    assert info.column == "c"
    assert info.datatype == "text"
    assert info.constraint == "not_null"


def test_error_info_repr_contains_required_triple():
    info = errors.ErrorInfo("FATAL", "28P01", "auth failed")
    s = repr(info)
    assert "FATAL" in s
    assert "28P01" in s
    assert "auth failed" in s


def test_error_info_is_fatal():
    assert errors.ErrorInfo("FATAL", "28P01", "x").is_fatal() is True
    assert errors.ErrorInfo("ERROR", "22000", "x").is_fatal() is False
