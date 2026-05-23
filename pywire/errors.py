"""Exception hierarchy for pywire.

Every variant of `pgwire::error::PgWireError` maps to a dedicated Python
exception class here, organized under two mid-level umbrellas
(`ProtocolError`, `AuthError`) and the root `Error`. `pgwire::error::ErrorInfo`
is mirrored as `ErrorInfo` and carries the standard PostgreSQL error-field
set (severity, code, message, optional detail/hint/position/...).

`IoError` does not appear under `pywire.errors`: pgwire's `IoError`
variant is flattened to Python's built-in `OSError`, which is what callers
catching around socket code already use.
"""

from pywire._pywire.errors import (
    ApiError,
    AuthError,
    Error,
    ErrorInfo,
    FailedToCoercePasswordMessage,
    FailedToParseParameter,
    InvalidAuthenticationMessageCode,
    InvalidCancelRequest,
    InvalidGssEncRequestMessage,
    InvalidMessageType,
    InvalidOauthMessage,
    InvalidOptionValue,
    InvalidPassword,
    InvalidRustTypeForParameter,
    InvalidSASLState,
    InvalidScramMessage,
    InvalidSecretKey,
    InvalidSSLRequestMessage,
    InvalidStartupMessage,
    InvalidTargetType,
    InvalidTransactionStatus,
    MessageTooLarge,
    NotReadyForQuery,
    OAuthAuthenticationFailed,
    OauthAuthzIdError,
    OAuthValidationError,
    ParameterIndexOutOfBound,
    PortalNotFound,
    PortalNotStarted,
    ProtocolError,
    QueryCanceled,
    StatementNotFound,
    UnsupportedCertificateSignatureAlgorithm,
    UnsupportedProtocolVersion,
    UnsupportedSASLAuthMethod,
    UserError,
    UserNameRequired,
)

__all__ = [
    "ApiError",
    "AuthError",
    "Error",
    "ErrorInfo",
    "FailedToCoercePasswordMessage",
    "FailedToParseParameter",
    "InvalidAuthenticationMessageCode",
    "InvalidCancelRequest",
    "InvalidGssEncRequestMessage",
    "InvalidMessageType",
    "InvalidOauthMessage",
    "InvalidOptionValue",
    "InvalidPassword",
    "InvalidRustTypeForParameter",
    "InvalidSASLState",
    "InvalidSSLRequestMessage",
    "InvalidScramMessage",
    "InvalidSecretKey",
    "InvalidStartupMessage",
    "InvalidTargetType",
    "InvalidTransactionStatus",
    "MessageTooLarge",
    "NotReadyForQuery",
    "OAuthAuthenticationFailed",
    "OAuthValidationError",
    "OauthAuthzIdError",
    "ParameterIndexOutOfBound",
    "PortalNotFound",
    "PortalNotStarted",
    "ProtocolError",
    "QueryCanceled",
    "StatementNotFound",
    "UnsupportedCertificateSignatureAlgorithm",
    "UnsupportedProtocolVersion",
    "UnsupportedSASLAuthMethod",
    "UserError",
    "UserNameRequired",
]
