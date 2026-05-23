"""Tests for `pywire.copy`."""

from __future__ import annotations

import pytest

import pywire
from pywire import copy


def test_copy_is_exposed_under_pywire():
    assert pywire.copy is copy


def test_copy_info_construction():
    info = copy.CopyInfo(direction="in", binary=False)
    assert info.direction == "in"
    assert info.binary is False
    assert info.column_formats == []
    # frozen
    with pytest.raises(AttributeError):
        info.direction = "out"  # type: ignore[misc]


def test_copy_info_with_formats():
    info = copy.CopyInfo(direction="out", binary=True, column_formats=[1, 1, 0])
    assert info.binary is True
    assert info.column_formats == [1, 1, 0]


def test_copy_handler_is_abstract():
    with pytest.raises(TypeError):
        copy.CopyHandler()  # type: ignore[abstract]


async def test_copy_handler_concrete_subclass_in_flow():
    received: list[bytes] = []

    class CopyIn(copy.CopyHandler):
        async def start_copy_in(self, query: str) -> copy.CopyInfo:
            return copy.CopyInfo(direction="in", binary=False)

        async def on_copy_data(self, chunk: bytes) -> None:
            received.append(chunk)

        async def on_copy_done(self) -> str:
            return f"COPY {len(received)}"

        async def start_copy_out(self, query: str) -> copy.CopyInfo:
            raise NotImplementedError

        async def next_copy_out_chunk(self) -> bytes:
            raise NotImplementedError

    h = CopyIn()
    info = await h.start_copy_in("COPY t FROM STDIN")
    assert info.direction == "in"
    await h.on_copy_data(b"alice\n")
    await h.on_copy_data(b"bob\n")
    tag = await h.on_copy_done()
    assert tag == "COPY 2"
    # default no-op:
    await h.on_copy_fail("test")


async def test_copy_handler_copy_out_flow():
    chunks = [b"row1\n", b"row2\n", b""]

    class CopyOut(copy.CopyHandler):
        def __init__(self) -> None:
            self._iter = iter(chunks)

        async def start_copy_in(self, query: str) -> copy.CopyInfo:
            raise NotImplementedError

        async def on_copy_data(self, chunk: bytes) -> None:
            raise NotImplementedError

        async def on_copy_done(self) -> str:
            raise NotImplementedError

        async def start_copy_out(self, query: str) -> copy.CopyInfo:
            return copy.CopyInfo(direction="out", binary=False)

        async def next_copy_out_chunk(self) -> bytes:
            return next(self._iter)

    h = CopyOut()
    info = await h.start_copy_out("COPY t TO STDOUT")
    assert info.direction == "out"
    seen: list[bytes] = []
    while True:
        ch = await h.next_copy_out_chunk()
        if not ch:
            break
        seen.append(ch)
    assert seen == [b"row1\n", b"row2\n"]
