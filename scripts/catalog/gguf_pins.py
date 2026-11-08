#!/usr/bin/env python3
"""Generate `crates/lattice-core/src/domain/models/curated/pins.rs`: for every curated GGUF, the
memory facts the runtime's reader (`gguf_metadata.rs`, `read_model_info`) reads from its header,
and the bytes of the file it keeps in system RAM (`cpu_weight_bytes`).

Only each header is fetched, by HTTP range requests that grow until the tensor table parses. The
rules below mirror the Rust reader exactly: a key is matched by what follows its first dot; an
integer or a list of at most 4096 integers is kept, and anything else is passed over; feed-forward
width is the widest layer's; `cpu_weight_bytes` is the distance from `token_embd.weight` to the next
tensor offset when the model has its own `output.weight`, else 0. The ignored test
`the_catalogs_pinned_shapes_and_cpu_weights_are_what_the_runtime_reader_reads` is the authority.

    python -X utf8 scripts/catalog/gguf_pins.py [--write PATH]

Standard library only.
"""

import argparse
import json
import os
import struct
import sys
import urllib.parse

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import gguf_footprint as base  # noqa: E402

MAX_PER_LAYER_VALUES = 4096
# The keys the Rust reader assigns even when the value is not a number or a list of them.
GENERIC = {"attention.head_count_kv", "attention.head_count", "feed_forward_length"}

# (repo, file, revision). Chat entries read `main`; the embedder and reranker are pinned to the
# commits their catalog entries download from.
FILES = [
    ("unsloth/Qwen3.5-2B-GGUF", "Qwen3.5-2B-Q4_K_M.gguf", "main"),
    ("unsloth/Qwen3.5-4B-GGUF", "Qwen3.5-4B-Q4_K_M.gguf", "main"),
    ("unsloth/gemma-4-E4B-it-GGUF", "gemma-4-E4B-it-Q4_K_M.gguf", "main"),
    ("unsloth/Qwen3.5-9B-GGUF", "Qwen3.5-9B-Q4_K_M.gguf", "main"),
    ("unsloth/Qwen3.5-9B-MTP-GGUF", "Qwen3.5-9B-Q4_K_M.gguf", "main"),
    ("unsloth/gemma-4-12b-it-GGUF", "gemma-4-12b-it-Q4_K_M.gguf", "main"),
    ("unsloth/Qwen3.5-9B-GGUF", "Qwen3.5-9B-Q6_K.gguf", "main"),
    ("unsloth/Qwen3.5-9B-GGUF", "Qwen3.5-9B-Q8_0.gguf", "main"),
    ("unsloth/gemma-4-12b-it-GGUF", "gemma-4-12b-it-Q6_K.gguf", "main"),
    ("byteshape/Qwen3.8-27B-GGUF", "Qwen3.8-27B-IQ3_S-3.23bpw.gguf", "main"),
    ("unsloth/gemma-4-26B-A4B-it-GGUF", "gemma-4-26B-A4B-it-UD-IQ3_S.gguf", "main"),
    ("unsloth/gemma-4-12b-it-GGUF", "gemma-4-12b-it-Q8_0.gguf", "main"),
    ("unsloth/Qwen3.8-27B-GGUF", "Qwen3.8-27B-UD-Q4_K_M.gguf", "main"),
    ("unsloth/gemma-4-26B-A4B-it-GGUF", "gemma-4-26B-A4B-it-UD-Q4_K_M.gguf", "main"),
    ("unsloth/Qwen3.6-35B-A3B-GGUF", "Qwen3.6-35B-A3B-UD-Q4_K_XL.gguf", "main"),
    (
        "Qwen/Qwen3-Embedding-0.6B-GGUF",
        "Qwen3-Embedding-0.6B-f16.gguf",
        "370f27d7550e0def9b39c1f16d3fbaa13aa67728",
    ),
    (
        "ggml-org/Qwen3-Reranker-0.6B-Q8_0-GGUF",
        "qwen3-reranker-0.6b-q8_0.gguf",
        "a02f48bb4f057028298c21fa033da2b30d7742d5",
    ),
]


def integer(value):
    """The Rust reader's `read_integer`: an int that fits u32, else None."""
    if isinstance(value, bool):
        value = int(value)
    if isinstance(value, int) and 0 <= value <= 0xFFFFFFFF:
        return value
    return None


def parse(data):
    """Parse the metadata and tensor table; return (facts, tensors, header_end)."""
    reader = base.Reader(data)
    if reader.take(4) != base.MAGIC:
        raise ValueError("not a GGUF file")
    version = reader.unpack("<I")
    if version not in (2, 3):
        raise ValueError(f"GGUF version {version}")
    tensor_count = reader.unpack("<Q")
    kv_count = reader.unpack("<Q")
    facts = {}
    for _ in range(kv_count):
        key = reader.string()
        kind = reader.unpack("<I")
        value = reader.value(kind)
        if key == "general.architecture" or key == "tokenizer.chat_template":
            continue
        if key == "general.file_type":
            continue
        if isinstance(value, list):
            numbers = None
            if len(value) <= MAX_PER_LAYER_VALUES and all(
                isinstance(v, (int, bool)) for v in value
            ):
                kept = [integer(v) for v in value]
                if all(v is not None for v in kept):
                    numbers = kept
            parsed = ("numbers", numbers) if numbers is not None else None
        elif isinstance(value, base.Skipped) or isinstance(value, (str, float)):
            parsed = None
        else:
            number = integer(value)
            parsed = ("number", number) if number is not None else None
        if "." not in key:
            continue
        name = key.split(".", 1)[1]
        if parsed is None and name not in GENERIC:
            continue
        facts[name] = parsed
    tensors = []
    for _ in range(tensor_count):
        name = reader.string()
        for _ in range(reader.unpack("<I")):
            reader.unpack("<Q")
        reader.unpack("<I")
        tensors.append((name, reader.unpack("<Q")))
    return facts, tensors, reader.pos


def read(fetch):
    size = base.FIRST_RANGE_BYTES
    while True:
        data = fetch(size)
        try:
            return parse(data)
        except base.NeedMore:
            if len(data) < size or size >= base.MAX_RANGE_BYTES:
                raise ValueError("the file ends inside its header")
            size *= 2


def number(facts, name):
    kept = facts.get(name)
    return kept[1] if kept and kept[0] == "number" else None


def per_layer(facts, name):
    kept = facts.get(name)
    if not kept:
        return None
    return kept


def widest(facts, name):
    kept = facts.get(name)
    if not kept:
        return None
    return kept[1] if kept[0] == "number" else max(kept[1])


def cpu_weight_bytes(tensors, header_end, file_size):
    offsets = [offset for _, offset in tensors]
    names = {name for name, _ in tensors}
    if "output.weight" not in names:
        return 0
    start = next((offset for name, offset in tensors if name == "token_embd.weight"), None)
    if start is None:
        return 0
    data_bytes = max(file_size - header_end, 0)
    end = min((o for o in offsets if o > start), default=data_bytes)
    return max(end - start, 0)


def opt(value):
    return "None" if value is None else f"Some({value})"


def heads(kept):
    if kept is None:
        raise ValueError("a header lacks a head count; every curated file has both")
    if kept[0] == "number":
        return f"Heads::Uniform({kept[1]})"
    return "Heads::Layers(&[" + ", ".join(str(v) for v in kept[1]) + "])"


def pattern(kept):
    if kept is None or kept[0] != "numbers":
        return "None"
    return "Some(&[" + ", ".join("true" if v else "false" for v in kept[1]) + "])"


def entry(repo, file, revision):
    size = base.tree_entry(repo, file, revision)["size"]
    facts, tensors, header_end = read(base.fetch_range(repo, file, revision))
    cpu = cpu_weight_bytes(tensors, header_end, size)
    return f"""    Pin {{
        repo: {json.dumps(repo)},
        file: {json.dumps(file)},
        block_count: {opt(number(facts, "block_count"))},
        head_count_kv: {heads(per_layer(facts, "attention.head_count_kv"))},
        head_count: {heads(per_layer(facts, "attention.head_count"))},
        key_length: {opt(number(facts, "attention.key_length"))},
        value_length: {opt(number(facts, "attention.value_length"))},
        key_length_swa: {opt(number(facts, "attention.key_length_swa"))},
        value_length_swa: {opt(number(facts, "attention.value_length_swa"))},
        embedding_length: {opt(number(facts, "embedding_length"))},
        sliding_window: {opt(number(facts, "attention.sliding_window"))},
        sliding_window_pattern: {pattern(facts.get("attention.sliding_window_pattern"))},
        full_attention_interval: {opt(number(facts, "full_attention_interval"))},
        nextn_predict_layers: {opt(number(facts, "nextn_predict_layers"))},
        shared_kv_layers: {opt(number(facts, "attention.shared_kv_layers"))},
        feed_forward_length: {opt(widest(facts, "feed_forward_length"))},
        ssm_conv_kernel: {opt(number(facts, "ssm.conv_kernel"))},
        ssm_state_size: {opt(number(facts, "ssm.state_size"))},
        ssm_group_count: {opt(number(facts, "ssm.group_count"))},
        ssm_inner_size: {opt(number(facts, "ssm.inner_size"))},
        cpu_weight_bytes: {cpu},
    }},
"""


HEADER = '''//! The memory facts pinned for every curated GGUF: what the runtime's reader reads from its header,
//! and the bytes of the file it keeps in system RAM. Generated by `scripts/catalog/gguf_pins.py`;
//! the ignored test `the_catalogs_pinned_shapes_and_cpu_weights_are_what_the_runtime_reader_reads`
//! checks every pin against the file's own header.

use crate::domain::models::fit::{ModelShape, PerLayer, PinnedModel};

/// A header value that is one number for every layer or one per layer.
pub(super) enum Heads {
    Uniform(u32),
    Layers(&'static [u32]),
}

impl Heads {
    fn per_layer(&self) -> PerLayer {
        match self {
            Self::Uniform(value) => PerLayer::Uniform(*value),
            Self::Layers(values) => PerLayer::Layers(values.to_vec()),
        }
    }
}

/// The pinned facts of one GGUF file.
pub(super) struct Pin {
    pub repo: &'static str,
    pub file: &'static str,
    block_count: Option<u32>,
    head_count_kv: Heads,
    head_count: Heads,
    key_length: Option<u32>,
    value_length: Option<u32>,
    key_length_swa: Option<u32>,
    value_length_swa: Option<u32>,
    embedding_length: Option<u32>,
    sliding_window: Option<u32>,
    sliding_window_pattern: Option<&'static [bool]>,
    full_attention_interval: Option<u32>,
    nextn_predict_layers: Option<u32>,
    shared_kv_layers: Option<u32>,
    feed_forward_length: Option<u32>,
    ssm_conv_kernel: Option<u32>,
    ssm_state_size: Option<u32>,
    ssm_group_count: Option<u32>,
    ssm_inner_size: Option<u32>,
    cpu_weight_bytes: u64,
}

impl Pin {
    pub(super) fn pinned(&self) -> PinnedModel {
        PinnedModel {
            shape: ModelShape {
                block_count: self.block_count,
                head_count_kv: Some(self.head_count_kv.per_layer()),
                head_count: Some(self.head_count.per_layer()),
                key_length: self.key_length,
                value_length: self.value_length,
                key_length_swa: self.key_length_swa,
                value_length_swa: self.value_length_swa,
                embedding_length: self.embedding_length,
                sliding_window: self.sliding_window,
                sliding_window_pattern: self.sliding_window_pattern.map(<[bool]>::to_vec),
                full_attention_interval: self.full_attention_interval,
                nextn_predict_layers: self.nextn_predict_layers,
                shared_kv_layers: self.shared_kv_layers,
                feed_forward_length: self.feed_forward_length,
                ssm_conv_kernel: self.ssm_conv_kernel,
                ssm_state_size: self.ssm_state_size,
                ssm_group_count: self.ssm_group_count,
                ssm_inner_size: self.ssm_inner_size,
            },
            cpu_weight_bytes: self.cpu_weight_bytes,
        }
    }
}

/// The pinned facts of `file` in `repo`, when the catalog pins it.
pub(super) fn pinned_for(repo: &str, file: &str) -> Option<PinnedModel> {
    PINS.iter()
        .find(|pin| pin.repo == repo && pin.file == file)
        .map(Pin::pinned)
}

const PINS: &[Pin] = &[
'''


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("--write")
    args = parser.parse_args(argv)
    out = HEADER
    for repo, file, revision in FILES:
        print(f"reading {repo}/{file}", file=sys.stderr)
        out += entry(repo, file, revision)
    out += "];\n"
    if args.write:
        with open(args.write, "w", encoding="utf-8", newline="") as handle:
            handle.write(out)
    else:
        sys.stdout.write(out)


if __name__ == "__main__":
    sys.exit(main())
