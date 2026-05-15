# Authentication

`pywire.auth` exposes the user-facing surface for plugging
authentication into a pywire server. You define your auth policy by
subclassing `AuthSource` and implementing an async `get_password`
method. pywire's startup handlers (cleartext / MD5 / SCRAM, shipping
with `pywire.server`) call your method during the connection handshake.

## The shape

```python
from pywire.auth import AuthSource, LoginInfo, Password
from pywire.errors import InvalidPassword


class StaticUsers(AuthSource):
    def __init__(self, users: dict[str, bytes]) -> None:
        self.users = users

    async def get_password(self, login: LoginInfo) -> Password:
        user = login.user or ""
        try:
            return Password(self.users[user])
        except KeyError:
            raise InvalidPassword(user) from None
```

Three types are involved:

| Type        | Purpose                                                                       |
| ----------- | ----------------------------------------------------------------------------- |
| `LoginInfo` | What we know about the client at auth time: `user`, `database`, `host`.       |
| `Password`  | The reference password. `salt` is `None` for cleartext, bytes for hashed.     |
| `AuthSource`| The async ABC you subclass.                                                   |

## Errors

Raise [`pywire.errors.InvalidPassword`](errors.md) (or any subclass of
`pywire.errors.AuthError`) to reject the connection. The PostgreSQL
client will see a standard auth-failed response. Any other exception is
surfaced as a server-side error.

## How `get_password` is called

When a client connects, the pywire server:

1. parses the startup message → constructs a `LoginInfo` from the
   `user`/`database` parameters and the connection's peer address,
2. calls your `AuthSource.get_password(login)` and awaits the
   coroutine,
3. compares the returned `Password.password` to what the client sent
   (after applying any salt + hashing required by the negotiated auth
   method),
4. sends `AuthenticationOk` and continues the handshake, or rejects
   the connection with the appropriate error.

The await happens inside pywire's tokio runtime. Long-running lookups
(database queries, LDAP, HTTP) are fine as long as they are themselves
async — they will not block other connections.

!!! warning "Server bindings not yet shipped"
    The startup handlers (`CleartextPasswordHandler`,
    `Md5PasswordHandler`, `SaslScramHandler`) and the high-level
    `pywire.serve(...)` entry point land with `pywire.server` (PR I).
    Today you can write your `AuthSource` subclass and have it be
    fully ready, but you can't yet stand up a running server with it.

## Reference

::: pywire.auth
    options:
      show_source: false
      heading_level: 3
      members_order: source
