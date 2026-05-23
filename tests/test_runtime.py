"""Tests for the async runtime bridge (`src/runtime.rs`).

These hit the bridge from both directions:

- Rust → Python: `_test_async_sleep` and `_test_async_add` are
  `#[pyfunction]`s that return Python awaitables built from Rust
  futures. Calling them and `await`-ing them from asyncio must yield
  the expected result.
- Python → Rust: `_test_await_python_coro` accepts a Python coroutine,
  drives it from a Rust async context via `pyo3_async_runtimes::tokio::
  into_future`, and returns the awaited value.

These functions are private — they exist for this test file only. As
real async bindings land (auth/query/server), each will exercise the
bridge in its own way.
"""

from __future__ import annotations

import asyncio
import time

import pytest

from pywire._pywire import (  # type: ignore[attr-defined]
    _test_async_add,
    _test_async_sleep,
    _test_await_python_coro,
)


async def test_async_sleep_returns_input_value():
    started = time.monotonic()
    result = await _test_async_sleep(0.05)
    elapsed = time.monotonic() - started
    assert result == 0.05
    # Allow generous slack so flaky CI scheduling doesn't flag this.
    assert elapsed >= 0.04


async def test_async_add_returns_sum_across_yields():
    assert await _test_async_add(3, 4) == 7


async def test_python_coro_round_trip():
    async def double(x: int) -> int:
        await asyncio.sleep(0)
        return x * 2

    assert await _test_await_python_coro(double(21)) == 42


async def test_python_coro_propagates_exception():
    async def boom() -> None:
        raise RuntimeError("kaboom")

    with pytest.raises(RuntimeError, match="kaboom"):
        await _test_await_python_coro(boom())


async def test_many_concurrent_sleeps():
    # Verify the runtime really is multi-threaded: ten 50ms sleeps run
    # concurrently should finish well under the serial 500ms total.
    started = time.monotonic()
    results = await asyncio.gather(*[_test_async_sleep(0.05) for _ in range(10)])
    elapsed = time.monotonic() - started
    assert results == [0.05] * 10
    assert elapsed < 0.4
