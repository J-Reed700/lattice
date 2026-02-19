import asyncio
from collections.abc import Callable
from dataclasses import dataclass, field
from datetime import datetime
import logging
from uuid import UUID

logger = logging.getLogger(__name__)


@dataclass
class IndexingTask:
    file_id: UUID
    priority: int
    retry_count: int = 0
    max_retries: int = 3
    created_at: datetime = field(default_factory=datetime.now)

    def __lt__(self, other):
        if self.priority != other.priority:
            return self.priority < other.priority
        return self.created_at < other.created_at


class BackgroundTaskQueue:
    def __init__(self, num_workers: int, db_session_factory: Callable):
        self.num_workers = num_workers
        self.db_session_factory = db_session_factory
        self.queue: asyncio.PriorityQueue = asyncio.PriorityQueue()
        self.workers: list[asyncio.Task] = []
        self.running = False

        self.stats = {"processed": 0, "failed": 0, "pending": 0}

    async def add_task(self, file_id: UUID, priority: int = 2) -> None:
        task = IndexingTask(file_id=file_id, priority=priority)
        await self.queue.put(task)
        self.stats["pending"] += 1
        logger.info(f"Added task to queue: file_id={file_id}, priority={priority}")

    async def start(self) -> None:
        if self.running:
            logger.warning("Task queue already running")
            return

        self.running = True
        self.workers = [asyncio.create_task(self._worker(i)) for i in range(self.num_workers)]
        logger.info(f"Started {self.num_workers} task queue workers")

    async def stop(self) -> None:
        if not self.running:
            return

        logger.info("Stopping task queue workers...")
        self.running = False

        for worker in self.workers:
            worker.cancel()

        await asyncio.gather(*self.workers, return_exceptions=True)
        logger.info("All workers stopped")

    async def _worker(self, worker_id: int) -> None:
        logger.info(f"Worker {worker_id} started")

        while self.running:
            try:
                task = await asyncio.wait_for(self.queue.get(), timeout=1.0)
                self.stats["pending"] -= 1

                logger.info(f"Worker {worker_id} processing file_id={task.file_id}")

                success = await self._process_task(task, worker_id)

                if success:
                    self.stats["processed"] += 1
                else:
                    self.stats["failed"] += 1

                self.queue.task_done()

            except TimeoutError:
                continue
            except asyncio.CancelledError:
                break
            except Exception as e:
                logger.error(f"Worker {worker_id} error: {e}", exc_info=True)

        logger.info(f"Worker {worker_id} stopped")

    async def _process_task(self, task: IndexingTask, worker_id: int) -> bool:
        from src.services.indexing import IndexingService

        async with self.db_session_factory() as db_session:
            indexing_service = IndexingService()

            while task.retry_count < task.max_retries:
                try:
                    success = await indexing_service.index_file(
                        file_id=task.file_id, db_session=db_session
                    )

                    if success:
                        logger.info(f"Worker {worker_id} completed file_id={task.file_id}")
                        return True
                    task.retry_count += 1
                    if task.retry_count < task.max_retries:
                        delay = 2 ** (task.retry_count - 1)
                        logger.warning(
                            f"Worker {worker_id} retry {task.retry_count}/{task.max_retries} "
                            f"for file_id={task.file_id} after {delay}s"
                        )
                        await asyncio.sleep(delay)
                    else:
                        logger.error(
                            f"Worker {worker_id} failed file_id={task.file_id} "
                            f"after {task.max_retries} retries"
                        )
                        return False

                except Exception as e:
                    task.retry_count += 1
                    logger.error(
                        f"Worker {worker_id} error processing file_id={task.file_id}: {e}",
                        exc_info=True,
                    )

                    if task.retry_count >= task.max_retries:
                        return False

                    delay = 2 ** (task.retry_count - 1)
                    await asyncio.sleep(delay)

            return False

    def get_queue_size(self) -> int:
        return self.queue.qsize()

    def get_stats(self) -> dict:
        return {**self.stats, "queue_size": self.get_queue_size()}
