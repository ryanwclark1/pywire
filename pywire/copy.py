"""COPY-protocol handler bindings for pywire.

PostgreSQL's bulk `COPY` protocol is a separate sub-protocol on top of
the connection: after a `COPY ... FROM STDIN` (or `TO STDOUT`) is
issued, the wire enters streaming mode where the frontend sends
`CopyData` / `CopyDone` / `CopyFail` messages until the operation
completes.

`CopyHandler` is the async ABC you subclass to define how your pywire
server starts, streams, and finishes a COPY. The connection-state
machine that drives the methods ships with `pywire.server` (PR I);
this module establishes the Python types.
"""

from __future__ import annotations

import abc
from dataclasses import dataclass, field


@dataclass(frozen=True)
class CopyInfo:
    """Metadata about an in-flight COPY operation.

    `direction` is `"in"` for `COPY FROM STDIN` (client → server) and
    `"out"` for `COPY TO STDOUT` (server → client). `binary` is True
    when the COPY uses binary format; False for text/CSV.
    `column_formats` is one format code per column (`0` text, `1`
    binary); the list is empty for `COPY ... TO STDOUT` of a whole
    relation in text format.
    """

    direction: str
    binary: bool
    column_formats: list[int] = field(default_factory=list)


class CopyHandler(abc.ABC):
    """Async ABC for `COPY ... FROM STDIN` and `COPY ... TO STDOUT`.

    For `COPY FROM STDIN` (client → server) the server calls:

      1. `start_copy_in(query) -> CopyInfo`
      2. `on_copy_data(chunk)` for each `CopyData` message the client
         sends. The chunk is opaque `bytes` per the COPY format the
         start_copy_in response negotiated.
      3. `on_copy_done()` when the client signals end-of-data.
         Returning a `command_tag` string (e.g. `"COPY 100"`) ends the
         operation.

    For `COPY TO STDOUT` (server → client) the server calls:

      1. `start_copy_out(query) -> CopyInfo` — also produce the first
         chunk if you can.
      2. `next_copy_out_chunk()` repeatedly. Return `b""` (empty
         bytes) to signal end-of-data and finalize with a
         `command_tag`.

    `on_copy_fail(message)` is called if the client aborts. Default:
    no-op; you may override to clean up partial state.
    """

    @abc.abstractmethod
    async def start_copy_in(self, query: str) -> CopyInfo:
        """Begin a `COPY FROM STDIN` operation."""

    @abc.abstractmethod
    async def on_copy_data(self, chunk: bytes) -> None:
        """Process one `CopyData` chunk from the client."""

    @abc.abstractmethod
    async def on_copy_done(self) -> str:
        """End of client-side data; return the wire command tag."""

    @abc.abstractmethod
    async def start_copy_out(self, query: str) -> CopyInfo:
        """Begin a `COPY TO STDOUT` operation."""

    @abc.abstractmethod
    async def next_copy_out_chunk(self) -> bytes:
        """Return the next chunk of `COPY TO STDOUT` data; `b""` ends."""

    async def on_copy_fail(self, message: str) -> None:  # noqa: B027 - default no-op
        """The client aborted the COPY. Default: no-op."""


__all__ = ["CopyHandler", "CopyInfo"]
