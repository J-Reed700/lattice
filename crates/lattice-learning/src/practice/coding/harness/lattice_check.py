import atexit
import contextlib
import io
import json
import os
import sys
import time

NONCE = "__NONCE__"
_failed = []


def _emit(name, status, printed, error, elapsed):
    line = json.dumps(
        {
            "caseId": name,
            "status": status,
            "actual": None,
            "printed": printed,
            "error": error,
            "elapsedMs": elapsed,
        }
    )
    sys.stdout.write(NONCE + line + "\n")
    sys.stdout.flush()


@contextlib.contextmanager
def case(name):
    sink = io.StringIO()
    started = time.perf_counter()
    status = "passed"
    error = None
    try:
        with contextlib.redirect_stdout(sink):
            yield
    except AssertionError as caught:
        status = "failed"
        error = str(caught) or None
    except BaseException as caught:
        status = "error"
        text = str(caught)
        error = type(caught).__name__ + ": " + text if text else type(caught).__name__
    elapsed = int((time.perf_counter() - started) * 1000)
    if status != "passed":
        _failed.append(name)
    _emit(name, status, sink.getvalue().rstrip("\r\n"), error, elapsed)


def _finish():
    if _failed:
        sys.stdout.flush()
        os._exit(1)


atexit.register(_finish)
