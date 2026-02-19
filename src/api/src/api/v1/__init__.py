"""API v1 routes."""

from fastapi import APIRouter

from src.api.v1 import (
    agent,
    agentic_rag,
    auth,
    clusters,
    csrf,
    documents,
    export,
    files,
    health,
    index,
    links,
    llm,
    mfa,
    monitoring,
    qa,
    rag,
    search,
    storage,
    summarize,
    sync,
    tags,
    watch,
)

router = APIRouter(prefix="/api/v1")

# Core routers
router.include_router(csrf.router)
router.include_router(auth.router)
router.include_router(mfa.router, prefix="/auth/mfa", tags=["mfa"])
router.include_router(sync.router)

# Document and file management
router.include_router(search.router)
router.include_router(files.router)
router.include_router(documents.router)
router.include_router(index.router)

# AI and agent features
router.include_router(agent.router)
router.include_router(agentic_rag.router)
router.include_router(llm.router)
router.include_router(qa.router)
router.include_router(rag.router)

# Supporting features
router.include_router(health.router)
router.include_router(monitoring.router)
router.include_router(export.router)
router.include_router(storage.router)
router.include_router(summarize.router)
router.include_router(tags.router)
router.include_router(clusters.router)
router.include_router(links.router)
router.include_router(watch.router)

__all__ = ["router"]
