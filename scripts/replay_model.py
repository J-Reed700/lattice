"""Which model a replay ran on, asked of the server and kept on every record.

A file name is not a record of the model: a result read back later has to say what wrote it, and two models in one file
are two results.
"""
import json
import os
import urllib.request


def served_model(url):
    """The one model the server at `url` says it is serving."""
    request = urllib.request.Request(url.rstrip("/") + "/v1/models")
    name, value = os.environ.get("LLAMA_AUTH_HEADER_NAME"), os.environ.get("LLAMA_AUTH_HEADER_VALUE")
    if name and value:
        request.add_header(name, value)
    with urllib.request.urlopen(request, timeout=30) as response:
        models = [model["id"] for model in json.load(response).get("data", [])]
    if len(models) != 1:
        raise SystemExit(f"expected the server to name one model, found {models}")
    return models[0]


def model_of(claude, url):
    """`claude`'s model when a replay runs on Claude, else what the server at `url` serves."""
    return claude.MODEL if claude else served_model(url)


def one_model(records):
    """The model `records` came from, refusing records that name none or more than one."""
    models = {record.get("model") for record in records}
    if len(models) != 1 or None in models:
        raise SystemExit(f"these records must name one model, found {sorted(map(str, models))}")
    return models.pop()
