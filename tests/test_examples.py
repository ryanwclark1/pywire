"""Every script in `examples/` runs and behaves as its docstring says.

The user guide includes these files verbatim, so these tests keep the
documentation honest.
"""

from __future__ import annotations

import asyncio
import contextlib
import importlib.util
import socket
import threading
import time
from collections.abc import AsyncIterator, Awaitable, Callable, Coroutine
from pathlib import Path
from types import ModuleType
from typing import Any

import psycopg
import pytest

from pywire._pywire import _test_bind_ephemeral

EXAMPLES = Path(__file__).resolve().parent.parent / "examples"
FIXTURES = Path(__file__).resolve().parent / "fixtures"


def load(name: str) -> ModuleType:
    spec = importlib.util.spec_from_file_location(f"pywire_example_{name}", EXAMPLES / f"{name}.py")
    assert spec is not None and spec.loader is not None
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


async def _wait_for(port: int) -> None:
    for _ in range(200):
        with contextlib.suppress(OSError):
            _, writer = await asyncio.open_connection("127.0.0.1", port)
            writer.close()
            await writer.wait_closed()
            return
        await asyncio.sleep(0.01)
    raise AssertionError(f"nothing listening on {port}")  # pragma: no cover


@contextlib.asynccontextmanager
async def serving(main: Callable[[str], Coroutine[Any, Any, None]]) -> AsyncIterator[str]:
    """Run `main(addr)` on a free port; yield a libpq conninfo prefix."""
    port = await _test_bind_ephemeral()
    task = asyncio.create_task(main(f"127.0.0.1:{port}"))
    await _wait_for(port)
    try:
        yield f"host=127.0.0.1 port={port}"
    finally:
        task.cancel()
        with contextlib.suppress(asyncio.CancelledError):
            await task


def run(fn: Callable[[], object]) -> Awaitable[object]:
    return asyncio.to_thread(fn)


def test_examples_directory_is_fully_tested() -> None:
    tested = {"hello", "error_handling", "streaming", "prepared_statements", "sessions"}
    tested |= {"copy_data", "auth_scram", "scram_verifier", "tls", "sync_server"}
    assert {path.stem for path in EXAMPLES.glob("*.py")} == tested


async def test_hello() -> None:
    async with serving(load("hello").main) as dsn:

        def check() -> None:
            with psycopg.connect(f"{dsn} sslmode=disable") as conn:
                assert conn.execute("SELECT 1").fetchall() == [("hello, world",)]
                assert conn.execute("SET x = 1").statusmessage == "SET"
                conn.commit()  # BEGIN ... COMMIT round trip
            with psycopg.connect(f"{dsn} sslmode=disable", autocommit=True) as conn:
                assert conn.execute("").statusmessage is None  # empty query

        await run(check)


async def test_error_handling() -> None:
    async with serving(load("error_handling").main) as dsn:

        def check() -> None:
            with psycopg.connect(f"{dsn} sslmode=disable", autocommit=True) as conn:
                assert conn.execute("SELECT 1").fetchall() == [(1,)]
                with pytest.raises(psycopg.errors.SyntaxError) as error:
                    conn.execute("DROP TABLE everything")
                assert error.value.diag.message_hint == "This server only understands SELECT 1."
                with pytest.raises(psycopg.errors.QueryCanceled):
                    conn.execute("CANCEL")
                with pytest.raises(psycopg.errors.UniqueViolation) as duplicate:
                    conn.execute("INSERT INTO items VALUES (1)")
                assert duplicate.value.diag.constraint_name == "items_pkey"

        await run(check)


async def test_streaming() -> None:
    async with serving(load("streaming").main) as dsn:

        def check() -> None:
            with psycopg.connect(f"{dsn} sslmode=disable", autocommit=True) as conn:
                rows = conn.execute("SELECT 2500").fetchall()
                assert len(rows) == 2500
                assert rows[:2] == [(1, None), (2, "even")]

        await run(check)


async def test_prepared_statements() -> None:
    async with serving(load("prepared_statements").main) as dsn:

        def check() -> None:
            with psycopg.connect(f"{dsn} sslmode=disable", autocommit=True) as conn:
                sql = "SELECT name FROM users WHERE id = %s"
                assert conn.execute(sql, [2]).fetchall() == [("bob",)]
                assert conn.execute(sql, ["1"]).fetchall() == [("alice",)]  # text param
                assert conn.execute(sql, [None]).fetchall() == []
                with conn.cursor(binary=True) as cursor:  # binary results
                    assert cursor.execute(sql, [1], prepare=True).fetchall() == [("alice",)]
                with pytest.raises(psycopg.errors.NoDataFound):
                    conn.execute(sql, [3])
                conn.execute("SET search_path = public")  # simple protocol

        await run(check)


async def test_sessions(capsys: pytest.CaptureFixture[str]) -> None:
    async with serving(load("sessions").main) as dsn:

        def check() -> None:
            with psycopg.connect(f"{dsn} user=ann sslmode=disable", autocommit=True) as conn:
                conn.execute("SELECT 1")
                assert conn.execute("SELECT 1").fetchall() == [("ann", 2)]
            with psycopg.connect(f"{dsn} user=bo sslmode=disable", autocommit=True) as conn:
                assert conn.execute("SELECT 1").fetchall() == [("bo", 1)]

        await run(check)
        for _ in range(50):
            if "bo disconnected" in capsys.readouterr().out:
                break
            await asyncio.sleep(0.02)
        else:  # pragma: no cover
            raise AssertionError("session close() was not called")
    with pytest.raises(NotImplementedError):
        await load("sessions").Unused().do_query("SELECT 1")


async def test_copy_data() -> None:
    async with serving(load("copy_data").main) as dsn:

        def check() -> None:
            with psycopg.connect(f"{dsn} sslmode=disable", autocommit=True) as conn:
                with conn.cursor() as cursor:
                    with cursor.copy("COPY items FROM STDIN") as copy:
                        copy.write(b"a1\tapple\nb2\tba")
                        copy.write(b"nana\n")
                    assert cursor.rowcount == 2
                    with pytest.raises(ZeroDivisionError), cursor.copy("COPY items FROM STDIN"):
                        raise ZeroDivisionError  # client-side abort: nothing loaded
                with psycopg.connect(f"{dsn} sslmode=disable", autocommit=True) as fresh:
                    with fresh.cursor().copy("COPY items TO STDOUT") as copy:
                        exported = b"".join(bytes(chunk) for chunk in copy)
                    assert exported == b"a1\tapple\nb2\tbanana\n"
                    assert fresh.execute("SELECT count(*) FROM items").fetchall() == [(2,)]
                    with pytest.raises(psycopg.errors.FeatureNotSupported):
                        fresh.execute("DELETE FROM items")

        await run(check)


@pytest.mark.parametrize("name", ["auth_scram", "scram_verifier"])
async def test_scram_examples(name: str) -> None:
    async with serving(load(name).main) as dsn:

        def check() -> None:
            good = f"{dsn} user=alice password=wonderland sslmode=disable"
            with psycopg.connect(good) as conn:
                assert conn.execute("SELECT 1").fetchone() is not None
            for user, password in [("alice", "nope"), ("mallory", "wonderland")]:
                with pytest.raises(psycopg.OperationalError, match="(?i)password"):
                    psycopg.connect(f"{dsn} user={user} password={password} sslmode=disable")

        await run(check)


async def test_tls(capsys: pytest.CaptureFixture[str], client_cert: str) -> None:
    tls = load("tls")
    cert, key = str(FIXTURES / "server.crt"), str(FIXTURES / "server.key")

    async def plain(addr: str) -> None:
        await tls.main(addr, cert=cert, key=key)

    async def mutual(addr: str) -> None:
        await tls.main(addr, cert=cert, key=key, client_ca=str(FIXTURES / "client-ca.crt"))

    for main, extra in [(plain, ""), (mutual, client_cert)]:
        async with serving(main) as dsn:

            def check(dsn: str = dsn, extra: str = extra) -> None:
                base = f"{dsn} user=alice password=wonderland"
                with pytest.raises(psycopg.OperationalError):
                    psycopg.connect(f"{base} sslmode=disable")
                with psycopg.connect(f"{base} sslmode=require {extra}") as conn:
                    assert conn.execute("SELECT 1").fetchall() == [(True,)]
                with pytest.raises(psycopg.OperationalError):
                    psycopg.connect(f"{dsn} user=eve password=x sslmode=require {extra}")

            await run(check)
    assert "1 certs" in capsys.readouterr().out


def test_sync_server() -> None:
    with socket.socket() as probe:
        probe.bind(("127.0.0.1", 0))
        port = probe.getsockname()[1]
    module = load("sync_server")
    threading.Thread(target=module.main, args=(f"127.0.0.1:{port}",), daemon=True).start()
    deadline = time.monotonic() + 5
    while True:
        try:
            with psycopg.connect(f"host=127.0.0.1 port={port} sslmode=disable") as conn:
                assert conn.execute("SELECT 1").fetchall() == [("hi",)]
                return
        except psycopg.OperationalError:  # pragma: no cover - still binding
            if time.monotonic() > deadline:
                raise
            time.sleep(0.05)
