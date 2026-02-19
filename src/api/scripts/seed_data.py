#!/usr/bin/env python
import asyncio
import sys
from pathlib import Path
from datetime import datetime

sys.path.insert(0, str(Path(__file__).parent.parent))

from sqlalchemy.ext.asyncio import create_async_engine, async_sessionmaker, AsyncSession
from src.config.settings import settings
from src.models.database import Base, File, FileContent, FileEmbedding


async def seed_sample_data():
    print(f"Seeding sample data to {settings.DATABASE_URL}")

    engine = create_async_engine(
        settings.DATABASE_URL,
        echo=True,
    )

    async_session = async_sessionmaker(
        engine,
        class_=AsyncSession,
        expire_on_commit=False,
    )

    async with async_session() as session:
        print("\nCreating sample files...")

        sample_files = [
            {
                "path": "/sample/documents/readme.txt",
                "filename": "readme.txt",
                "file_type": "text/plain",
                "size": 1024,
                "hash": "abc123",
                "content": "This is a sample README file for testing purposes.",
            },
            {
                "path": "/sample/documents/notes.md",
                "filename": "notes.md",
                "file_type": "text/markdown",
                "size": 2048,
                "hash": "def456",
                "content": "# Sample Notes\n\nThese are sample notes for the Vault system.",
            },
            {
                "path": "/sample/images/screenshot.png",
                "filename": "screenshot.png",
                "file_type": "image/png",
                "size": 102400,
                "hash": "ghi789",
                "content": None,
            },
        ]

        for file_data in sample_files:
            content = file_data.pop("content")

            file = File(
                **file_data,
                created_at=datetime.utcnow(),
                modified_at=datetime.utcnow(),
                last_indexed=datetime.utcnow(),
            )

            session.add(file)
            await session.flush()

            if content:
                file_content = FileContent(
                    file_id=file.id,
                    content=content,
                    extracted_text=content,
                )
                session.add(file_content)

            print(f"  ✓ Created: {file.path}")

        await session.commit()

    await engine.dispose()
    print("\n✓ Sample data seeded successfully!")


if __name__ == "__main__":
    asyncio.run(seed_sample_data())
