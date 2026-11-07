"""Tests for gguf_footprint.py: the header parser and the KV arithmetic.

    python -X utf8 -I -m unittest discover -s scripts/catalog -p "test_*.py"
"""

import os
import struct
import sys
import unittest

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))

import gguf_footprint as g  # noqa: E402


def s(text):
    raw = text.encode("utf-8")
    return struct.pack("<Q", len(raw)) + raw


def u32(kind, value):
    return struct.pack("<I", kind) + struct.pack("<I", value)


def kv_u32(key, value):
    return s(key) + u32(g.T_U32, value)


def kv_str(key, value):
    return s(key) + struct.pack("<I", g.T_STRING) + s(value)


def kv_array(key, element, fmt, values):
    body = b"".join(struct.pack(fmt, v) for v in values)
    return (
        s(key)
        + struct.pack("<I", g.T_ARRAY)
        + struct.pack("<I", element)
        + struct.pack("<Q", len(values))
        + body
    )


def kv_string_array(key, values):
    return (
        s(key)
        + struct.pack("<I", g.T_ARRAY)
        + struct.pack("<I", g.T_STRING)
        + struct.pack("<Q", len(values))
        + b"".join(s(v) for v in values)
    )


def tensor(name, dims):
    return (
        s(name)
        + struct.pack("<I", len(dims))
        + b"".join(struct.pack("<Q", d) for d in dims)
        + struct.pack("<I", 0)
        + struct.pack("<Q", 0)
    )


def gguf(kvs, tensors=()):
    return (
        b"GGUF"
        + struct.pack("<I", 3)
        + struct.pack("<Q", len(tensors))
        + struct.pack("<Q", len(kvs))
        + b"".join(kvs)
        + b"".join(tensors)
    )


def qwen_header(blocks, kv_heads, key_len, interval, nextn=0):
    kvs = [
        kv_str("general.architecture", "qwen35"),
        kv_u32("qwen35.block_count", blocks),
        kv_u32("qwen35.attention.head_count_kv", kv_heads),
        kv_u32("qwen35.attention.key_length", key_len),
        kv_u32("qwen35.attention.value_length", key_len),
        kv_u32("qwen35.full_attention_interval", interval),
        kv_u32("qwen35.context_length", 262144),
    ]
    if nextn:
        kvs.append(kv_u32("qwen35.nextn_predict_layers", nextn))
    return kvs


class ParserTests(unittest.TestCase):
    def test_reads_scalars_strings_and_the_tensor_table(self):
        data = gguf(
            [kv_str("general.architecture", "llama"), kv_u32("llama.block_count", 7)],
            [tensor("a", [3, 4]), tensor("b", [5])],
        )
        version, tensors, metadata, elements = g.parse_header(data)
        self.assertEqual(version, 3)
        self.assertEqual(tensors, 2)
        self.assertEqual(metadata["general.architecture"], "llama")
        self.assertEqual(metadata["llama.block_count"], 7)
        self.assertEqual(elements, 3 * 4 + 5)

    def test_keeps_short_numeric_arrays_and_walks_long_ones(self):
        long_tokens = [f"t{i}" for i in range(g.KEPT_ARRAY_LIMIT + 10)]
        data = gguf(
            [
                kv_array("x.heads", g.T_U32, "<I", [8, 8, 1]),
                kv_string_array("tokenizer.ggml.tokens", long_tokens),
                kv_array("x.scores", g.T_F32, "<f", [0.5] * (g.KEPT_ARRAY_LIMIT + 1)),
                kv_u32("x.after", 42),
            ]
        )
        _, _, metadata, _ = g.parse_header(data)
        self.assertEqual(metadata["x.heads"], [8, 8, 1])
        self.assertIsInstance(metadata["tokenizer.ggml.tokens"], g.Skipped)
        self.assertEqual(metadata["tokenizer.ggml.tokens"].length, len(long_tokens))
        self.assertIsInstance(metadata["x.scores"], g.Skipped)
        self.assertEqual(metadata["x.after"], 42)

    def test_a_truncated_buffer_asks_for_more_at_every_cut(self):
        data = gguf(
            [kv_str("general.architecture", "llama"), kv_string_array("t", ["ab", "cd"])],
            [tensor("a", [2])],
        )
        for cut in range(len(data)):
            with self.assertRaises(g.NeedMore, msg=f"cut at {cut}"):
                g.parse_header(data[:cut])

    def test_rejects_a_file_that_is_not_gguf(self):
        with self.assertRaises(ValueError):
            g.parse_header(b"NOPE" + b"\0" * 64)

    def test_read_header_grows_the_range_until_it_parses(self):
        padding = [f"token-{i}" for i in range(300_000)]
        data = gguf(
            [kv_string_array("tokenizer.ggml.tokens", padding), kv_u32("x.last", 9)]
        )
        self.assertGreater(len(data), 2 * g.FIRST_RANGE_BYTES)
        asked = []

        def fetch(n):
            asked.append(n)
            return data[:n]

        _, _, metadata, _ = g.read_header(fetch)
        self.assertEqual(metadata["x.last"], 9)
        self.assertGreater(len(asked), 1)
        self.assertEqual(asked, sorted(asked))

    def test_read_header_fails_when_the_file_really_ends_early(self):
        data = gguf([kv_str("general.architecture", "llama")])[:-3]
        with self.assertRaises(ValueError):
            g.read_header(lambda n: data[:n])


class FootprintTests(unittest.TestCase):
    def footprint(self, kvs):
        _, _, metadata, _ = g.parse_header(gguf(kvs))
        return g.footprint(metadata)

    def test_one_in_four_full_attention_layers(self):
        shape = self.footprint(qwen_header(blocks=8, kv_heads=2, key_len=128, interval=4))
        self.assertEqual(shape["full_attention_layers"], 2)
        self.assertEqual(shape["per_token_bytes"], 2 * 2 * (128 + 128) * 2)
        self.assertEqual(shape["windowed_bytes"], 0)

    def test_an_mtp_block_adds_a_full_attention_layer(self):
        without = self.footprint(qwen_header(32, 4, 256, 4))
        with_mtp = self.footprint(qwen_header(33, 4, 256, 4, nextn=1))
        self.assertEqual(with_mtp["per_token_bytes"] - without["per_token_bytes"], 4 * 512 * 2)

    def test_sliding_layers_are_capped_at_the_window_plus_a_batch(self):
        pattern = [True, True, False, True, True, False]
        heads = [4, 4, 1, 4, 4, 1]
        shape = self.footprint(
            [
                kv_str("general.architecture", "gemma4"),
                kv_u32("gemma4.block_count", 6),
                kv_array("gemma4.attention.head_count_kv", g.T_U32, "<I", heads),
                kv_u32("gemma4.attention.key_length", 64),
                kv_u32("gemma4.attention.value_length", 64),
                kv_u32("gemma4.attention.key_length_swa", 32),
                kv_u32("gemma4.attention.value_length_swa", 32),
                kv_u32("gemma4.attention.sliding_window", 100),
                kv_array("gemma4.attention.sliding_window_pattern", g.T_BOOL, "<?", pattern),
            ]
        )
        self.assertEqual(shape["full_attention_layers"], 2)
        self.assertEqual(shape["sliding_window_layers"], 4)
        self.assertEqual(shape["per_token_bytes"], 2 * (1 * 128 * 2))
        self.assertEqual(shape["windowed_bytes"], 4 * (4 * 64 * 2) * (100 + 512))

    def test_layers_that_reuse_an_earlier_cache_keep_none(self):
        shape = self.footprint(
            [
                kv_str("general.architecture", "gemma4"),
                kv_u32("gemma4.block_count", 4),
                kv_u32("gemma4.attention.head_count_kv", 2),
                kv_u32("gemma4.attention.key_length", 16),
                kv_u32("gemma4.attention.value_length", 16),
                kv_u32("gemma4.attention.shared_kv_layers", 2),
            ]
        )
        self.assertEqual(shape["full_attention_layers"], 2)

    def test_an_integer_pattern_means_every_nth_layer_is_global(self):
        shape = self.footprint(
            [
                kv_str("general.architecture", "gemma4"),
                kv_u32("gemma4.block_count", 12),
                kv_u32("gemma4.attention.head_count_kv", 2),
                kv_u32("gemma4.attention.key_length", 16),
                kv_u32("gemma4.attention.value_length", 16),
                kv_u32("gemma4.attention.sliding_window", 64),
                kv_u32("gemma4.attention.sliding_window_pattern", 6),
            ]
        )
        self.assertEqual(shape["full_attention_layers"], 2)
        self.assertEqual(shape["sliding_window_layers"], 10)


class ExistingEntryTests(unittest.TestCase):
    """Header fields copied from the pinned files of two catalog entries,
    against the numbers `curated/chat.rs` already carries for them."""

    def test_qwen_3_5_9b_matches_qwen35_4b_9b_kv(self):
        # unsloth/Qwen3.5-9B-GGUF Qwen3.5-9B-Q4_K_M.gguf
        shape = ExistingEntryTests.shape(
            [
                kv_str("general.architecture", "qwen35"),
                kv_u32("qwen35.block_count", 32),
                kv_u32("qwen35.attention.head_count", 16),
                kv_u32("qwen35.attention.head_count_kv", 4),
                kv_u32("qwen35.attention.key_length", 256),
                kv_u32("qwen35.attention.value_length", 256),
                kv_u32("qwen35.full_attention_interval", 4),
            ]
        )
        # QWEN35_4B_9B_KV: 8 * 4 * 512 * 2, no windowed part.
        self.assertEqual(shape["per_token_bytes"], 8 * 4 * 512 * 2)
        self.assertEqual(shape["windowed_bytes"], 0)

    def test_gemma_4_12b_matches_its_catalog_entry(self):
        # unsloth/gemma-4-12b-it-GGUF gemma-4-12b-it-Q4_K_M.gguf
        heads = ([8] * 5 + [1]) * 8
        pattern = ([True] * 5 + [False]) * 8
        shape = ExistingEntryTests.shape(
            [
                kv_str("general.architecture", "gemma4"),
                kv_u32("gemma4.block_count", 48),
                kv_u32("gemma4.attention.head_count", 16),
                kv_array("gemma4.attention.head_count_kv", g.T_U32, "<I", heads),
                kv_u32("gemma4.attention.key_length", 512),
                kv_u32("gemma4.attention.value_length", 512),
                kv_u32("gemma4.attention.key_length_swa", 256),
                kv_u32("gemma4.attention.value_length_swa", 256),
                kv_u32("gemma4.attention.sliding_window", 1024),
                kv_u32("gemma4.attention.shared_kv_layers", 0),
                kv_array("gemma4.attention.sliding_window_pattern", g.T_BOOL, "<?", pattern),
            ]
        )
        # per_token_bytes: 8 * 1024 * 2; windowed_bytes: 40 * 8 * 512 * 2 * (1024 + 512).
        self.assertEqual(shape["per_token_bytes"], 8 * 1024 * 2)
        self.assertEqual(shape["windowed_bytes"], 40 * 8 * 512 * 2 * (1024 + 512))

    @staticmethod
    def shape(kvs):
        _, _, metadata, _ = g.parse_header(gguf(kvs))
        return g.footprint(metadata)


if __name__ == "__main__":
    unittest.main()
