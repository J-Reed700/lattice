import logging

from sqlalchemy import text

from ..models.database import Base
from .connection import get_engine

logger = logging.getLogger(__name__)


async def enable_extensions() -> None:
    engine = get_engine()

    async with engine.begin() as conn:
        await conn.execute(text("CREATE EXTENSION IF NOT EXISTS vector"))
        await conn.execute(text("CREATE EXTENSION IF NOT EXISTS pg_trgm"))
        await conn.execute(text("CREATE EXTENSION IF NOT EXISTS btree_gin"))

    logger.info("Database extensions enabled")


async def create_tables() -> None:
    engine = get_engine()

    async with engine.begin() as conn:
        await conn.run_sync(Base.metadata.create_all)

    logger.info("Database tables created")


async def drop_tables() -> None:
    engine = get_engine()

    async with engine.begin() as conn:
        await conn.run_sync(Base.metadata.drop_all)

    logger.info("Database tables dropped")


async def init_db(create_extensions: bool = True, create_schema: bool = True) -> None:
    if create_extensions:
        await enable_extensions()

    if create_schema:
        await create_tables()

    logger.info("Database initialized successfully")


async def reset_db() -> None:
    await drop_tables()
    await init_db(create_extensions=False, create_schema=True)
    logger.info("Database reset completed")
