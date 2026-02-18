import fnmatch
from pathlib import Path


class PathFilter:
    DEFAULT_IGNORE_PATTERNS = [
        ".git",
        ".git/*",
        "node_modules",
        "node_modules/*",
        "__pycache__",
        "__pycache__/*",
        "*.pyc",
        "*.pyo",
        "*.pyd",
        ".DS_Store",
        "Thumbs.db",
        "*.tmp",
        "*.temp",
        "*.swp",
        "~*",
    ]

    def __init__(self, ignore_patterns: list[str] | None = None):
        self.ignore_patterns = ignore_patterns or self.DEFAULT_IGNORE_PATTERNS

    def should_process(self, file_path: Path) -> bool:
        if not file_path.exists() or not file_path.is_file():
            return False

        path_str = str(file_path)
        for pattern in self.ignore_patterns:
            if fnmatch.fnmatch(path_str, pattern):
                return False
            for part in file_path.parts:
                if fnmatch.fnmatch(part, pattern):
                    return False

        return True
