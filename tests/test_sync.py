"""Tests for `pywire.sync`."""

from __future__ import annotations

import threading
import time

import pytest

import pywire
from pywire import query, sync


def test_sync_is_exposed_under_pywire():
    assert pywire.sync is sync


class _Empty(query.SimpleQueryHandler):
    async def do_query(self, q: str) -> list[query.Response]:
        return [query.Response.empty()]


def test_serve_forever_propagates_unrelated_errors():
    """`serve_forever` only suppresses KeyboardInterrupt / CancelledError;
    a bad address surfaces normally."""
    with pytest.raises(ValueError):
        sync.serve_forever(_Empty(), "not-an-address")


def test_serve_forever_runs_serve_in_its_own_loop():
    """End-to-end sanity: spawn `serve_forever` in a thread and verify
    it accepts a TCP connection on the configured address. The thread
    is daemon, so it gets cleaned up when pytest exits."""
    import socket

    # Pick a free port using the stdlib (we can't call our async
    # _test_bind_ephemeral from sync code without a running loop).
    with socket.socket() as probe:
        probe.bind(("127.0.0.1", 0))
        port = probe.getsockname()[1]
    addr = f"127.0.0.1:{port}"

    def worker() -> None:
        sync.serve_forever(_Empty(), addr)

    t = threading.Thread(target=worker, daemon=True)
    t.start()
    # Wait for the listener to bind.
    deadline = time.monotonic() + 2.0
    last_err: Exception | None = None
    while time.monotonic() < deadline:
        try:
            with socket.create_connection(("127.0.0.1", port), timeout=0.5) as s:
                s.close()
                break
        except OSError as exc:
            last_err = exc
            time.sleep(0.05)
    else:
        raise AssertionError(f"serve_forever never bound {addr}: {last_err}")
