from pywire.auth import AuthSource
from pywire.query import SimpleQueryHandler

__all__: list[str]

async def serve(
    simple_query: SimpleQueryHandler,
    addr: str,
    *,
    auth: AuthSource | None = None,
) -> None: ...
