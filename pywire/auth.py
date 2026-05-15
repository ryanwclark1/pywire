"""Authentication-policy bindings for pywire.

A user defines authentication for a pywire server by subclassing
`AuthSource` and implementing the async `get_password` callback:

```python
from pywire.auth import AuthSource, LoginInfo, Password

class StaticUsers(AuthSource):
    def __init__(self, users: dict[str, bytes]) -> None:
        self.users = users

    async def get_password(self, login: LoginInfo) -> Password:
        user = login.user or ""
        try:
            return Password(self.users[user])
        except KeyError:
            from pywire.errors import InvalidPassword
            raise InvalidPassword(user) from None
```

The handler classes themselves (cleartext / MD5 / SCRAM startup
handlers) ship with `pywire.server` (PR I); they consume an
`AuthSource` instance per connection.

`LoginInfo` and `Password` are pyclass-shaped and immutable. They form
the contract between the Rust side (pgwire's startup handler) and the
Python side (your auth policy).
"""

from __future__ import annotations

import abc

from pywire._pywire.auth import LoginInfo, Password


class AuthSource(abc.ABC):
    """Abstract async source of password material.

    Subclass and implement `get_password` to teach a pywire server how
    to look up credentials for the connecting user. The method is
    awaited inside pywire's tokio runtime; long-running lookups (DB
    fetches, LDAP, HTTP) are fine — they will not block the runtime so
    long as they themselves are async.
    """

    @abc.abstractmethod
    async def get_password(self, login: LoginInfo) -> Password:
        """Return the password material for `login`.

        Raise `pywire.errors.InvalidPassword` (or any subclass of
        `pywire.errors.AuthError`) to reject the connection. Any other
        exception is surfaced to the client as a server-side error.
        """


__all__ = ["AuthSource", "LoginInfo", "Password"]
