from dataclasses import dataclass
import hashlib
import json
from typing import Any


def make_cache_key(*args: Any, **kwargs: Any) -> str:
    key_data = {
        "args": [
            str(arg) if not isinstance(arg, (dict, list)) else json.dumps(arg, sort_keys=True)
            for arg in args
        ],
        "kwargs": {
            k: str(v) if not isinstance(v, (dict, list)) else json.dumps(v, sort_keys=True)
            for k, v in sorted(kwargs.items())
        },
    }
    key_string = json.dumps(key_data, sort_keys=True)
    return hashlib.sha256(key_string.encode()).hexdigest()


def make_embedding_key(text: str, model_name: str) -> str:
    return hashlib.sha256(f"{text}:{model_name}".encode()).hexdigest()


def make_search_key(query: str, params: dict[str, Any]) -> str:
    params_str = json.dumps(params, sort_keys=True)
    return hashlib.sha256(f"{query}:{params_str}".encode()).hexdigest()


@dataclass
class CacheStats:
    cache_type: str
    hits: int
    misses: int
    size: int
    max_size: int
    evictions: int
    memory_bytes: int = 0

    @property
    def hit_rate(self) -> float:
        total = self.hits + self.misses
        return self.hits / total if total > 0 else 0.0

    @property
    def utilization(self) -> float:
        return self.size / self.max_size if self.max_size > 0 else 0.0

    def to_dict(self) -> dict[str, Any]:
        return {
            "cache_type": self.cache_type,
            "hits": self.hits,
            "misses": self.misses,
            "size": self.size,
            "max_size": self.max_size,
            "evictions": self.evictions,
            "hit_rate": self.hit_rate,
            "utilization": self.utilization,
            "memory_mb": self.memory_bytes / (1024 * 1024),
        }
