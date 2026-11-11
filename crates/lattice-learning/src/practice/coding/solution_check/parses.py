import ast
import os
import sys

BASE = os.path.dirname(globals().get("__file__") or "")
with open(os.path.join(BASE, "snippet.txt"), encoding="utf-8") as handle:
    source = handle.read()
try:
    ast.parse(source)
except Exception:
    sys.exit(1)
