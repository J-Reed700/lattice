import ast
import json
import os

BASE = os.path.dirname(globals().get("__file__") or "")
with open(os.path.join(BASE, "literals.json"), encoding="utf-8") as handle:
    sources = json.load(handle)

problems = []
for source in sources:
    try:
        ast.literal_eval(source)
        problems.append(None)
    except Exception as error:
        problems.append(type(error).__name__ + ": " + str(error))
print(json.dumps(problems))
