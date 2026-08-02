import getpass
import sys


_MAX_PASSWORD_BYTES = 1024


def _paperworks_getpass(prompt="", stream=None):
    value = sys.stdin.buffer.readline(_MAX_PASSWORD_BYTES + 2)
    if not value.endswith(b"\n"):
        raise RuntimeError("Protected PDF password input was unavailable.")
    value = value[:-1]
    if value.endswith(b"\r"):
        value = value[:-1]
    if len(value) > _MAX_PASSWORD_BYTES:
        raise RuntimeError("Protected PDF password input exceeded its safety limit.")
    try:
        return value.decode("utf-8")
    except UnicodeDecodeError as error:
        raise RuntimeError("Protected PDF password input was not valid UTF-8.") from error


getpass.getpass = _paperworks_getpass
