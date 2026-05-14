import pywire


def test_supported_protocol_range() -> None:
    assert pywire.supported_protocol_range() == (3, 3)
