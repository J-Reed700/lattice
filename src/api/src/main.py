"""
Main entry point for Vault API server.

This module starts the FastAPI application using uvicorn.
"""

import logging

import uvicorn

from src.config import get_settings

logging.basicConfig(
    level=logging.INFO, format="%(asctime)s - %(name)s - %(levelname)s - %(message)s"
)

logger = logging.getLogger(__name__)


def main():
    """
    Start the Vault API server.
    """
    settings = get_settings()

    logger.info("Starting Vault API server...")
    logger.info(f"Host: {settings.api_host}")
    logger.info(f"Port: {settings.api_port}")
    logger.info(f"Workers: {settings.api_workers}")
    logger.info(f"Reload: {settings.api_reload}")
    logger.info(f"Debug: {settings.debug}")

    uvicorn.run(
        "src.api.app:app",
        host=settings.api_host,
        port=settings.api_port,
        reload=settings.api_reload,
        workers=1 if settings.api_reload else settings.api_workers,
        log_level=settings.log_level.lower(),
        access_log=True,
    )


if __name__ == "__main__":
    main()
