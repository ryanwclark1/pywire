from hypothesis import given
from hypothesis import strategies as st

import pywire


def test_supported_protocol_range_value() -> None:
    assert pywire.supported_protocol_range() == (3, 3)


def test_supported_protocol_range_invariants() -> None:
    earliest, latest = pywire.supported_protocol_range()
    assert isinstance(earliest, int)
    assert isinstance(latest, int)
    assert 0 <= earliest <= 0xFFFF, "earliest must fit in a u16"
    assert 0 <= latest <= 0xFFFF, "latest must fit in a u16"
    assert earliest <= latest, "earliest must be <= latest"


@given(iterations=st.integers(min_value=0, max_value=50))
def test_supported_protocol_range_is_pure(iterations: int) -> None:
    expected = pywire.supported_protocol_range()
    for _ in range(iterations):
        assert pywire.supported_protocol_range() == expected
