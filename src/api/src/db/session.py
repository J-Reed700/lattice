from collections.abc import AsyncGenerator
from contextlib import asynccontextmanager
import logging

from sqlalchemy.ext.asyncio import AsyncSession, async_sessionmaker

from .connection import get_engine

logger = logging.getLogger(__name__)

_session_factory: async_sessionmaker[AsyncSession] | None = None


def get_session_factory() -> async_sessionmaker[AsyncSession]:
    global _session_factory

    if _session_factory is not None:
        return _session_factory

    engine = get_engine()

    _session_factory = async_sessionmaker(
        engine,
        class_=AsyncSession,
        expire_on_commit=False,
        autocommit=False,
        autoflush=False,
    )

    logger.debug("Session factory created")

    return _session_factory


async def get_session() -> AsyncGenerator[AsyncSession, None]:
    session_factory = get_session_factory()

    async with session_factory() as session:
        try:
            yield session
        except Exception as e:
            await session.rollback()
            logger.error(f"Session error, rolling back: {e}")
            raise
        finally:
            await session.close()


@asynccontextmanager
async def session_context() -> AsyncGenerator[AsyncSession, None]:
    session_factory = get_session_factory()

    async with session_factory() as session:
        try:
            yield session
            await session.commit()
        except Exception as e:
            await session.rollback()
            logger.error(f"Session context error, rolling back: {e}")
            raise
