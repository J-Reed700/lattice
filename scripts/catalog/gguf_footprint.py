#!/usr/bin/env python3
"""Read a GGUF file's header from Hugging Face and print what a curated catalog
entry needs: the pinned size and SHA-256, the architecture's KV shape, and the
KV footprint counted the way `curated/chat.rs` counts it.

Only the header is fetched, by HTTP range requests that grow until the whole
metadata block parses. The weights are never downloaded.

    python -X utf8 -I scripts/catalog/gguf_footprint.py REPO FILE [--revision main]

Standard library only.
"""

import argparse
import json
import struct
import sys
import urllib.parse
import urllib.request

MAGIC = b"GGUF"
BATCH_TOKENS = 512
FIRST_RANGE_BYTES = 1 << 20
MAX_RANGE_BYTES = 256 << 20
# Arrays longer than this (tokenizer vocabularies) are walked, not kept.
KEPT_ARRAY_LIMIT = 4096

T_U8, T_I8, T_U16, T_I16, T_U32, T_I32, T_F32, T_BOOL = range(8)
T_STRING, T_ARRAY, T_U64, T_I64, T_F64 = 8, 9, 10, 11, 12
SCALAR = {
    T_U8: "<B", T_I8: "<b", T_U16: "<H", T_I16: "<h", T_U32: "<I",
    T_I32: "<i", T_F32: "<f", T_BOOL: "<?", T_U64: "<Q", T_I64: "<q",
    T_F64: "<d",
}


class NeedMore(Exception):
    """The buffer ends before the metadata does."""


class Skipped:
    """Stands in for a long array that was walked but not kept."""

    def __init__(self, length):
        self.length = length

    def __repr__(self):
        return f"<array of {self.length}>"


class Reader:
    def __init__(self, data):
        self.data = data
        self.pos = 0

    def take(self, n):
        end = self.pos + n
        if end > len(self.data):
            raise NeedMore(end)
        chunk = self.data[self.pos:end]
        self.pos = end
        return chunk

    def unpack(self, fmt):
        size = struct.calcsize(fmt)
        return struct.unpack(fmt, self.take(size))[0]

    def string(self):
        return self.take(self.unpack("<Q")).decode("utf-8", "replace")

    def skip_string(self):
        length = self.unpack("<Q")
        end = self.pos + length
        if end > len(self.data):
            raise NeedMore(end)
        self.pos = end

    def value(self, kind):
        if kind in SCALAR:
            return self.unpack(SCALAR[kind])
        if kind == T_STRING:
            return self.string()
        if kind == T_ARRAY:
            return self.array()
        raise ValueError(f"unknown GGUF value type {kind}")

    def array(self):
        element = self.unpack("<I")
        length = self.unpack("<Q")
        keep = length <= KEPT_ARRAY_LIMIT
        if element in SCALAR:
            width = struct.calcsize(SCALAR[element])
            raw = self.take(width * length)
            if not keep:
                return Skipped(length)
            fmt = "<" + str(length) + SCALAR[element][1]
            return list(struct.unpack(fmt, raw))
        if element == T_STRING:
            if keep:
                return [self.string() for _ in range(length)]
            for _ in range(length):
                self.skip_string()
            return Skipped(length)
        if element == T_ARRAY:
            if keep:
                return [self.array() for _ in range(length)]
            for _ in range(length):
                self.array()
            return Skipped(length)
        raise ValueError(f"unknown GGUF array element type {element}")


def parse_header(data):
    """Parse a GGUF buffer into (version, tensor_count, metadata, tensor_elements).

    tensor_elements is the sum of every tensor's element count, from the
    tensor table that follows the metadata.

    Raises NeedMore when the buffer ends inside the metadata.
    """
    reader = Reader(data)
    if reader.take(4) != MAGIC:
        raise ValueError("not a GGUF file")
    version = reader.unpack("<I")
    if version < 2:
        raise ValueError(f"GGUF version {version} is not supported")
    tensor_count = reader.unpack("<Q")
    kv_count = reader.unpack("<Q")
    metadata = {}
    for _ in range(kv_count):
        key = reader.string()
        kind = reader.unpack("<I")
        metadata[key] = reader.value(kind)
    parameters = 0
    for _ in range(tensor_count):
        reader.skip_string()
        dims = [reader.unpack("<Q") for _ in range(reader.unpack("<I"))]
        reader.take(4 + 8)
        elements = 1
        for dim in dims:
            elements *= dim
        parameters += elements
    return version, tensor_count, metadata, parameters


def read_header(fetch):
    """Grow the range until the metadata parses. `fetch(n)` returns the first n bytes (or fewer)."""
    size = FIRST_RANGE_BYTES
    while True:
        data = fetch(size)
        try:
            return parse_header(data)
        except NeedMore:
            if len(data) < size or size >= MAX_RANGE_BYTES:
                raise ValueError("the file ends inside its metadata")
            size *= 2


def per_layer(value, blocks):
    """A header field that is a scalar or one entry per block, as one entry per block."""
    if value is None:
        return [0] * blocks
    if isinstance(value, list):
        if len(value) < blocks:
            raise ValueError(f"per-layer array has {len(value)} entries for {blocks} blocks")
        return list(value[:blocks])
    return [value] * blocks


def footprint(metadata):
    """The catalog's KV rule for one header: returns a dict of the shape and the two byte counts.

    per_token_bytes: full-attention layers, key+value bytes per token at f16.
    windowed_bytes: sliding-window layers, key+value bytes at (window + 512).
    Layers an MTP head appends (`nextn_predict_layers`) attend over the whole
    window and are counted as full attention, as the catalog's MTP entry does.
    """
    arch = metadata["general.architecture"]

    def get(name):
        return metadata.get(f"{arch}.{name}")

    blocks = get("block_count")
    nextn = get("nextn_predict_layers") or 0
    heads_kv = per_layer(get("attention.head_count_kv"), blocks)
    key_len = get("attention.key_length")
    value_len = get("attention.value_length")
    window = get("attention.sliding_window")
    pattern = get("attention.sliding_window_pattern")
    interval = get("full_attention_interval")
    shared = get("attention.shared_kv_layers") or 0

    if key_len is None:
        heads = get("attention.head_count")
        embedding = get("embedding_length")
        if heads is None or embedding is None:
            raise ValueError("header has no attention key length")
        key_len = embedding // (max(heads) if isinstance(heads, list) else heads)
    if value_len is None:
        value_len = key_len
    key_len_swa = get("attention.key_length_swa") or key_len
    value_len_swa = get("attention.value_length_swa") or value_len

    if isinstance(pattern, list):
        sliding = [bool(flag) for flag in pattern[:blocks]]
    elif isinstance(pattern, int) and not isinstance(pattern, bool) and pattern > 0:
        sliding = [(i % pattern) < (pattern - 1) for i in range(blocks)]
    else:
        sliding = [False] * blocks
    if not window:
        sliding = [False] * blocks
    if interval:
        recurrent = [(i + 1) % interval != 0 for i in range(blocks)]
    else:
        recurrent = [False] * blocks
    # Layers at the end that reuse an earlier layer's cache keep none of their own.
    keeps = [i < blocks - shared for i in range(blocks)]

    full = []
    swa = []
    for i in range(blocks):
        if recurrent[i] or not keeps[i] or not heads_kv[i]:
            continue
        if sliding[i]:
            swa.append(heads_kv[i] * (key_len_swa + value_len_swa) * 2)
        else:
            full.append(heads_kv[i] * (key_len + value_len) * 2)
    if nextn:
        last_heads = heads_kv[-1]
        full.extend([last_heads * (key_len + value_len) * 2] * nextn)

    per_token = sum(full)
    windowed = sum(swa) * ((window or 0) + BATCH_TOKENS)
    return {
        "architecture": arch,
        "block_count": blocks,
        "nextn_predict_layers": nextn,
        "full_attention_layers": len(full),
        "sliding_window_layers": len(swa),
        "head_count_kv": sorted(set(h for h in heads_kv if h)),
        "key_length": key_len,
        "value_length": value_len,
        "key_length_swa": key_len_swa,
        "value_length_swa": value_len_swa,
        "sliding_window": window,
        "full_attention_interval": interval,
        "shared_kv_layers": shared,
        "context_length": get("context_length"),
        "per_token_bytes": per_token,
        "windowed_bytes": windowed,
    }


def http_get(url, headers=None):
    request = urllib.request.Request(url, headers=headers or {})
    with urllib.request.urlopen(request, timeout=120) as response:
        return response.read(), response.status


def tree_entry(repo, file, revision):
    """size and lfs.oid (the SHA-256) of one file, from the tree API."""
    folder = file.rpartition("/")[0]
    url = f"https://huggingface.co/api/models/{repo}/tree/{urllib.parse.quote(revision, safe='')}"
    if folder:
        url += "/" + urllib.parse.quote(folder)
    body, _ = http_get(url)
    for entry in json.loads(body):
        if entry.get("path") == file:
            lfs = entry.get("lfs") or {}
            return {"size": entry.get("size"), "sha256": lfs.get("oid")}
    raise ValueError(f"{file} is not in {repo}@{revision}")


def commit_sha(repo, revision):
    body, _ = http_get(
        f"https://huggingface.co/api/models/{repo}/revision/{urllib.parse.quote(revision, safe='')}"
    )
    return json.loads(body).get("sha")


def fetch_range(repo, file, revision):
    url = (
        f"https://huggingface.co/{repo}/resolve/{urllib.parse.quote(revision, safe='')}/"
        f"{urllib.parse.quote(file)}"
    )

    def fetch(n):
        data, status = http_get(url, {"Range": f"bytes=0-{n - 1}"})
        if status == 200 and len(data) > n:
            raise RuntimeError("server ignored the range request")
        return data

    return fetch


def parameter_count(metadata):
    if "general.parameter_count" in metadata:
        return metadata["general.parameter_count"]
    return None


def describe(repo, file, revision):
    entry = tree_entry(repo, file, revision)
    version, tensors, metadata, elements = read_header(fetch_range(repo, file, revision))
    shape = footprint(metadata)
    arch = shape["architecture"]
    return {
        "repo": repo,
        "file": file,
        "revision": revision,
        "commit": commit_sha(repo, revision),
        "size_bytes": entry["size"],
        "sha256": entry["sha256"],
        "gguf_version": version,
        "tensor_count": tensors,
        "general.name": metadata.get("general.name"),
        "general.size_label": metadata.get("general.size_label"),
        "parameter_count": parameter_count(metadata),
        "tensor_elements": elements,
        "file_type": metadata.get("general.file_type"),
        "raw": {
            k: v
            for k, v in metadata.items()
            if k.startswith(arch + ".") and not isinstance(v, Skipped)
        },
        **shape,
    }


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("repo")
    parser.add_argument("file")
    parser.add_argument("--revision", default="main")
    args = parser.parse_args(argv)
    print(json.dumps(describe(args.repo, args.file, args.revision), indent=2))


if __name__ == "__main__":
    sys.exit(main())
