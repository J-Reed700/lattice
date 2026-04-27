"""Clusters API endpoints for document clustering and topic organization.

This module provides REST API endpoints for:
- Creating and updating document clusters
- Listing clusters and their members
- Cluster visualization data
- Hierarchical cluster navigation
"""

from __future__ import annotations

import logging
from typing import Any
from uuid import UUID

from fastapi import APIRouter, BackgroundTasks, Depends, HTTPException, Query
import numpy as np
from pydantic import BaseModel, Field
from sqlalchemy import select
from sqlalchemy.ext.asyncio import AsyncSession

from src.api.dependencies import get_db
from src.auth.dependencies import get_current_active_user
from src.auth.models import User
from src.middleware.csrf import csrf_protect
from src.models import Cluster, ClusterAlgorithm, File, FileCluster, TextEmbedding
from src.modules.clusterer import DocumentClusterer

logger = logging.getLogger(__name__)

router = APIRouter(prefix="/clusters", tags=["clusters"])


class CreateClusterRequest(BaseModel):
    """Request model for creating clusters."""

    algorithm: str = Field(default="hdbscan", pattern="^(hdbscan|kmeans|agglomerative)$")
    n_clusters: int | None = Field(default=None, gt=0, le=100)
    min_cluster_size: int = Field(default=5, gt=0)
    file_ids: list[UUID] | None = None


class ClusterResponse(BaseModel):
    """Response model for a cluster."""

    id: UUID
    name: str
    algorithm: ClusterAlgorithm
    cluster_label: int
    size: int
    density: float
    is_outlier: bool


class FileClusterResponse(BaseModel):
    """Response model for file-cluster association."""

    file_id: UUID
    cluster_id: UUID
    cluster_name: str
    membership_score: float
    distance: float | None
    is_core: bool


@router.post("/create", response_model=dict[str, Any])
async def create_clusters(
    request: CreateClusterRequest,
    background_tasks: BackgroundTasks,
    current_user: User = Depends(get_current_active_user),
    _: None = Depends(csrf_protect),
    db: AsyncSession = Depends(get_db),
):
    """Create document clusters using specified algorithm.

    Args:
        request: Clustering configuration
        background_tasks: FastAPI background tasks
        db: Database session

    Returns:
        Clustering results with cluster information
    """
    try:
        if request.file_ids:
            stmt = (
                select(File, TextEmbedding)
                .join(TextEmbedding.text_content)
                .join(TextEmbedding.text_content.file)
                .where(File.id.in_(request.file_ids))
            )
        else:
            stmt = (
                select(File, TextEmbedding)
                .join(TextEmbedding.text_content)
                .join(TextEmbedding.text_content.file)
                .limit(1000)
            )

        result = await db.execute(stmt)
        rows = result.all()

        if not rows:
            raise HTTPException(status_code=404, detail="No files with embeddings found")

        embeddings_dict = {}
        for file, embedding in rows:
            if file.id not in embeddings_dict:
                embeddings_dict[file.id] = []
            embeddings_dict[file.id].append(embedding.embedding)

        doc_ids = list(embeddings_dict.keys())
        embeddings = np.array([np.mean(embeddings_dict[doc_id], axis=0) for doc_id in doc_ids])

        clusterer = DocumentClusterer(
            algorithm=request.algorithm,
            n_clusters=request.n_clusters,
            min_cluster_size=request.min_cluster_size,
            enable_visualization=True,
        )

        cluster_result = await clusterer.cluster_documents(
            embeddings=embeddings, doc_ids=[str(did) for did in doc_ids]
        )

        for cluster_info in cluster_result["clusters"]:
            cluster = Cluster(
                name=cluster_info.get("name", f"Cluster {cluster_info['cluster_id']}"),
                algorithm=ClusterAlgorithm(request.algorithm),
                cluster_label=cluster_info["cluster_id"],
                centroid=cluster_info.get("centroid"),
                size=cluster_info["size"],
                density=cluster_info["density"],
                is_outlier=cluster_info.get("is_outlier", False),
            )
            db.add(cluster)
            await db.flush()

            for doc_id_str in cluster_info["doc_ids"]:
                doc_id = UUID(doc_id_str)
                doc_ids.index(doc_id)

                file_cluster = FileCluster(
                    file_id=doc_id, cluster_id=cluster.id, membership_score=0.8, is_core=True
                )
                db.add(file_cluster)

        await db.commit()

        return {
            "status": "success",
            "algorithm": request.algorithm,
            "n_clusters": cluster_result["n_clusters"],
            "clusters": cluster_result["clusters"],
            "visualization": cluster_result.get("visualization"),
        }

    except HTTPException:
        raise
    except Exception as e:
        logger.error(f"Error creating clusters: {e}")
        await db.rollback()
        raise HTTPException(status_code=500, detail=str(e))


@router.get("/list", response_model=list[ClusterResponse])
async def list_clusters(
    algorithm: ClusterAlgorithm | None = Query(default=None),
    min_size: int = Query(default=0, ge=0),
    skip: int = Query(default=0, ge=0),
    limit: int = Query(default=50, gt=0, le=200),
    current_user: User = Depends(get_current_active_user),
    db: AsyncSession = Depends(get_db),
):
    """List all clusters with optional filters.

    Args:
        algorithm: Filter by algorithm
        min_size: Minimum cluster size
        skip: Number of records to skip
        limit: Maximum number of records
        db: Database session

    Returns:
        List of clusters
    """
    try:
        stmt = select(Cluster).where(Cluster.size >= min_size)

        if algorithm:
            stmt = stmt.where(Cluster.algorithm == algorithm)

        stmt = stmt.order_by(Cluster.size.desc()).offset(skip).limit(limit)

        result = await db.execute(stmt)
        clusters = result.scalars().all()

        return [
            ClusterResponse(
                id=c.id,
                name=c.name,
                algorithm=c.algorithm,
                cluster_label=c.cluster_label,
                size=c.size,
                density=c.density,
                is_outlier=c.is_outlier,
            )
            for c in clusters
        ]

    except Exception as e:
        logger.error(f"Error listing clusters: {e}")
        raise HTTPException(status_code=500, detail=str(e))


@router.get("/{cluster_id}/documents", response_model=list[FileClusterResponse])
async def get_cluster_documents(
    cluster_id: UUID,
    current_user: User = Depends(get_current_active_user),
    db: AsyncSession = Depends(get_db),
):
    """Get all documents in a cluster.

    Args:
        cluster_id: Cluster ID
        db: Database session

    Returns:
        List of documents in the cluster
    """
    try:
        result = await db.execute(select(Cluster).where(Cluster.id == cluster_id))
        cluster = result.scalar_one_or_none()

        if not cluster:
            raise HTTPException(status_code=404, detail="Cluster not found")

        stmt = (
            select(FileCluster)
            .where(FileCluster.cluster_id == cluster_id)
            .order_by(FileCluster.membership_score.desc())
        )

        result = await db.execute(stmt)
        file_clusters = result.scalars().all()

        return [
            FileClusterResponse(
                file_id=fc.file_id,
                cluster_id=cluster_id,
                cluster_name=cluster.name,
                membership_score=fc.membership_score,
                distance=fc.distance,
                is_core=fc.is_core,
            )
            for fc in file_clusters
        ]

    except HTTPException:
        raise
    except Exception as e:
        logger.error(f"Error getting cluster documents: {e}")
        raise HTTPException(status_code=500, detail=str(e))
