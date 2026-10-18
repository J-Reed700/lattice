"""Exercise real model loading and CPU generation with the upstream tiny fixture.

Usage: python3 smoke_llama_cpu.py /path/to/llama-server /path/to/stories260K.gguf
Fixture: https://huggingface.co/ggml-org/models-moved/blob/main/tinyllamas/stories260K.gguf
This tests execution, not useful model quality or real-world performance.
"""

import hashlib
import json
from pathlib import Path
import socket
import subprocess
import sys
import tempfile
import time
import urllib.error
import urllib.request


def main():
    binary, model = (Path(arg).resolve() for arg in sys.argv[1:])
    expected = "270cba1bd5109f42d03350f60406024560464db173c0e387d91f0426d3bd256d"
    if hashlib.sha256(model.read_bytes()).hexdigest() != expected:
        raise RuntimeError("The CPU smoke fixture does not match its pinned SHA-256")
    with socket.socket() as sock:
        sock.bind(("127.0.0.1", 0))
        port = sock.getsockname()[1]
    # Never send local requests through a developer or runner's proxy.
    client = urllib.request.build_opener(urllib.request.ProxyHandler({}))
    base = f"http://127.0.0.1:{port}"
    with tempfile.TemporaryDirectory(prefix="lattice-cpu-smoke-") as directory:
        log = Path(directory) / "server.log"
        with log.open("w") as output:
            process = subprocess.Popen([
                str(binary), "-m", str(model), "--host", "127.0.0.1", "--port", str(port),
                "-ngl", "0", "-c", "128", "-np", "1", "-t", "2",
            ], cwd=directory, stdout=output, stderr=subprocess.STDOUT)
        try:
            deadline = time.monotonic() + 60
            while True:
                if process.poll() is not None:
                    raise RuntimeError(f"Server exited before readiness: {process.returncode}")
                try:
                    with client.open(base + "/health", timeout=2) as response:
                        ready = json.load(response).get("status") == "ok"
                    if ready:
                        break
                except (urllib.error.URLError, TimeoutError):
                    pass
                if time.monotonic() >= deadline:
                    raise RuntimeError("CPU model did not become ready within 60 seconds")
                time.sleep(0.1)
            request = urllib.request.Request(base + "/completion", data=json.dumps({
                "prompt": "Once upon a time", "n_predict": 8, "temperature": 0,
                "seed": 1, "stream": False,
            }).encode(), headers={"Content-Type": "application/json"})
            with client.open(request, timeout=30) as response:
                result = json.load(response)
            count = result.get("tokens_predicted", 0)
            if not result.get("content") or count < 1:
                raise RuntimeError(f"No generated text/tokens: {result}")
            print(json.dumps({"cpu_generation": "passed", "tokens_predicted": count,
                              "content": result["content"]}))
        except Exception:
            print(log.read_text(), file=sys.stderr)
            raise
        finally:
            if process.poll() is None:
                process.terminate()
                try:
                    process.wait(timeout=10)
                except subprocess.TimeoutExpired:
                    process.kill()
                    process.wait(timeout=10)


if __name__ == "__main__":
    main()
