#!/usr/bin/env python
import asyncio
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).parent.parent))

from sqlalchemy.ext.asyncio import create_async_engine
from src.config.settings import settings
from src.models.database import Base


async def init_database():
    print(f"Initializing database at {settings.DATABASE_URL}")

    engine = create_async_engine(
        settings.DATABASE_URL,
        echo=True,
    )

    async with engine.begin() as conn:
        print("Creating tables...")
        await conn.run_sync(Base.metadata.create_all)
        print("Tables created successfully!")

        print("Creating pgvector extension...")
        await conn.execute("CREATE EXTENSION IF NOT EXISTS vector;")
        print("pgvector extension created!")

    await engine.dispose()
    print("Database initialization complete!")


if __name__ == "__main__":
    asyncio.run(init_database())
