"""Shared pytest fixtures."""

from __future__ import annotations

import shutil
from pathlib import Path

import pytest

FIXTURES = Path(__file__).resolve().parent / "fixtures"


@pytest.fixture
def client_cert(tmp_path: Path) -> str:
    """libpq conninfo options that present the test client certificate.

    libpq refuses a private key that group or others can read, and a git
    checkout leaves `client.key` at 0644, so the key is copied to a private
    temporary file first. Paths use forward slashes because libpq treats
    a backslash in a conninfo value as an escape (Windows paths would be
    mangled and the certificate silently skipped).
    """
    key = tmp_path / "client.key"
    shutil.copyfile(FIXTURES / "client.key", key)
    key.chmod(0o600)
    return f"sslcert={(FIXTURES / 'client.crt').as_posix()} sslkey={key.as_posix()}"
