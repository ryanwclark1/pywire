from typing import Any

from pywire.query import SimpleQueryHandler

__all__: list[str]

async def serve(simple_query: SimpleQueryHandler, addr: str) -> Any: ...
