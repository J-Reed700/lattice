import asyncio
from collections.abc import Awaitable, Callable
import logging

from .event_queue import DebouncedEventQueue
from .types import FileEvent

logger = logging.getLogger(__name__)


class FileProcessorWorkerPool:
    def __init__(self, num_workers: int, process_callback: Callable[[FileEvent], Awaitable[None]]):
        self.num_workers = num_workers
        self.process_callback = process_callback
        self.workers: list[asyncio.Task] = []
        self.running = False

    async def start(self, event_queue: DebouncedEventQueue) -> None:
        self.running = True
        self.workers = [
            asyncio.create_task(self._worker(i, event_queue)) for i in range(self.num_workers)
        ]
        logger.info(f"Started {self.num_workers} file processor workers")

    async def stop(self) -> None:
        self.running = False
        for worker in self.workers:
            worker.cancel()
        await asyncio.gather(*self.workers, return_exceptions=True)
        logger.info("Stopped all file processor workers")

    async def _worker(self, worker_id: int, event_queue: DebouncedEventQueue) -> None:
        logger.info(f"Worker {worker_id} started")

        while self.running:
            try:
                event = await event_queue.get_event()
                logger.debug(f"Worker {worker_id} processing: {event.file_path}")

                await self._process_with_retry(event)

            except asyncio.CancelledError:
                break
            except Exception as e:
                logger.error(f"Worker {worker_id} error: {e}", exc_info=True)

        logger.info(f"Worker {worker_id} stopped")

    async def _process_with_retry(self, event: FileEvent) -> None:
        max_retries = 3
        base_delay = 1.0

        while event.retry_count < max_retries:
            try:
                await self.process_callback(event)
                return
            except Exception as e:
                event.retry_count += 1
                if event.retry_count >= max_retries:
                    logger.error(
                        f"Failed to process {event.file_path} after {max_retries} retries: {e}"
                    )
                    raise

                delay = base_delay * (2 ** (event.retry_count - 1))
                logger.warning(
                    f"Retry {event.retry_count}/{max_retries} for {event.file_path} "
                    f"after {delay}s delay: {e}"
                )
                await asyncio.sleep(delay)
