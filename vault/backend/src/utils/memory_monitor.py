"""Memory monitoring and cleanup utilities for preventing OOM crashes.

This module provides tools to monitor memory usage and trigger automatic
cleanup when thresholds are exceeded, preventing memory leaks from causing
crashes during long-running operations like batch image/PDF processing.
"""

import gc
import logging
import os

logger = logging.getLogger(__name__)


class MemoryMonitor:
    """Monitor and manage memory usage with automatic cleanup.

    Tracks memory usage and automatically triggers garbage collection
    and GPU cache cleanup when memory thresholds are exceeded.

    Attributes:
        threshold_mb: Memory threshold in MB, triggers cleanup if exceeded
        process: psutil Process instance for monitoring

    Example:
        >>> monitor = MemoryMonitor(threshold_mb=8192)
        >>> monitor.log_memory("before processing")
        >>> monitor.check_and_cleanup()
        >>> monitor.log_memory("after cleanup")
    """

    def __init__(self, threshold_mb: int = 8192):
        """Initialize memory monitor.

        Args:
            threshold_mb: Memory threshold in MB, trigger cleanup if exceeded
        """
        self.threshold_mb = threshold_mb
        self._process = None

    @property
    def process(self):
        """Lazy load psutil.Process to avoid import errors."""
        if self._process is None:
            try:
                import psutil

                self._process = psutil.Process(os.getpid())
            except ImportError:
                logger.warning(
                    "psutil not installed, memory monitoring disabled. "
                    "Install with: pip install psutil"
                )
        return self._process

    def get_memory_mb(self) -> float | None:
        """Get current memory usage in MB.

        Returns:
            Current memory usage in MB, or None if psutil unavailable
        """
        if self.process is None:
            return None

        try:
            return self.process.memory_info().rss / 1024 / 1024
        except Exception as e:
            logger.warning(f"Failed to get memory info: {e}")
            return None

    def check_and_cleanup(self) -> bool:
        """Check memory and cleanup if threshold exceeded.

        Returns:
            True if cleanup was triggered, False otherwise
        """
        current_memory = self.get_memory_mb()

        if current_memory is None:
            return False

        if current_memory > self.threshold_mb:
            logger.warning(
                f"Memory usage high: {current_memory:.1f} MB "
                f"(threshold: {self.threshold_mb} MB). Running cleanup..."
            )
            self.force_cleanup()

            new_memory = self.get_memory_mb()
            if new_memory is not None:
                freed = current_memory - new_memory
                logger.info(f"Cleanup freed {freed:.1f} MB, now at {new_memory:.1f} MB")

            return True

        return False

    def force_cleanup(self) -> None:
        """Force garbage collection and GPU cache cleanup.

        Triggers Python garbage collection and clears PyTorch CUDA
        cache if available. Call this manually to free memory.
        """
        gc.collect()

        try:
            import torch

            if torch.cuda.is_available():
                torch.cuda.empty_cache()
                torch.cuda.synchronize()
        except ImportError:
            pass

    def log_memory(self, label: str = "") -> None:
        """Log current memory usage with optional label.

        Args:
            label: Descriptive label for the log entry
        """
        memory_mb = self.get_memory_mb()
        if memory_mb is not None:
            logger.info(f"Memory {label}: {memory_mb:.1f} MB")


_global_monitor: MemoryMonitor | None = None


def get_memory_monitor(threshold_mb: int = 8192) -> MemoryMonitor:
    """Get or create global memory monitor singleton.

    Args:
        threshold_mb: Memory threshold in MB (only used on first call)

    Returns:
        MemoryMonitor instance
    """
    global _global_monitor

    if _global_monitor is None:
        _global_monitor = MemoryMonitor(threshold_mb=threshold_mb)

    return _global_monitor


def cleanup_memory() -> None:
    """Convenience function for manual memory cleanup.

    Triggers garbage collection and GPU cache cleanup.
    Can be called from anywhere to free memory.
    """
    monitor = get_memory_monitor()
    monitor.force_cleanup()


def check_memory(threshold_mb: int = 8192) -> bool:
    """Check memory and auto-cleanup if needed.

    Args:
        threshold_mb: Memory threshold in MB

    Returns:
        True if cleanup was triggered, False otherwise
    """
    monitor = get_memory_monitor(threshold_mb=threshold_mb)
    return monitor.check_and_cleanup()


def log_memory(label: str = "") -> None:
    """Log current memory usage.

    Args:
        label: Descriptive label for the log entry
    """
    monitor = get_memory_monitor()
    monitor.log_memory(label)
