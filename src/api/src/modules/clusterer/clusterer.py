"""Main document clustering orchestrator.

This module provides a unified interface for document clustering
using multiple algorithms.

Contract:
    Input: Document embeddings + algorithm choice
    Output: Cluster assignments with metadata
    Performance: 1-3 seconds for 1000 documents
"""

import asyncio
from concurrent.futures import ThreadPoolExecutor
from typing import Any

import numpy as np

from .agglomerative_clusterer import AgglomerativeClusterer
from .hdbscan_clusterer import HDBSCANClusterer
from .kmeans_clusterer import KMeansClusterer
from .umap_visualizer import UMAPVisualizer


class DocumentClusterer:
    """Unified document clustering system.

    Orchestrates multiple clustering algorithms and provides
    a consistent interface for document clustering.

    Attributes:
        algorithm: Clustering algorithm ('hdbscan', 'kmeans', 'agglomerative')
        n_clusters: Number of clusters (for kmeans/agglomerative)
        min_cluster_size: Minimum cluster size
        enable_visualization: Generate UMAP visualizations

    Example:
        >>> clusterer = DocumentClusterer(algorithm='hdbscan')
        >>> result = await clusterer.cluster_documents(
        ...     embeddings=embeddings,
        ...     doc_ids=doc_ids,
        ...     doc_titles=titles
        ... )
        >>> print(result['clusters'])
        [
            {
                "cluster_id": 0,
                "name": "Machine Learning",
                "size": 42,
                "doc_ids": [...],
                "centroid": [...]
            },
            ...
        ]
    """

    def __init__(
        self,
        algorithm: str = "hdbscan",
        n_clusters: int | None = None,
        min_cluster_size: int = 5,
        enable_visualization: bool = True,
    ):
        """Initialize document clusterer.

        Args:
            algorithm: Clustering algorithm
            n_clusters: Number of clusters (for kmeans/agglomerative)
            min_cluster_size: Minimum cluster size
            enable_visualization: Generate UMAP visualizations
        """
        self.algorithm = algorithm
        self.n_clusters = n_clusters
        self.min_cluster_size = min_cluster_size
        self.enable_visualization = enable_visualization

        self.clusterer = self._create_clusterer()

        self.visualizer = None
        if enable_visualization:
            try:
                self.visualizer = UMAPVisualizer(n_components=2)
            except ImportError:
                self.visualizer = None

        self.executor = ThreadPoolExecutor(max_workers=2)

    def _create_clusterer(self):
        """Create clusterer based on algorithm choice."""
        if self.algorithm == "hdbscan":
            return HDBSCANClusterer(min_cluster_size=self.min_cluster_size)
        if self.algorithm == "kmeans":
            if not self.n_clusters:
                self.n_clusters = 10
            return KMeansClusterer(n_clusters=self.n_clusters)
        if self.algorithm == "agglomerative":
            return AgglomerativeClusterer(n_clusters=self.n_clusters)
        raise ValueError(f"Unknown algorithm: {self.algorithm}")

    async def cluster_documents(
        self,
        embeddings: np.ndarray,
        doc_ids: list[str],
        doc_titles: list[str] | None = None,
        texts: list[str] | None = None,
    ) -> dict[str, Any]:
        """Cluster documents and generate comprehensive results.

        Args:
            embeddings: Document embeddings (n_docs x embedding_dim)
            doc_ids: Document IDs
            doc_titles: Optional document titles
            texts: Optional document texts for cluster naming

        Returns:
            Dictionary with clustering results

        Raises:
            ValueError: If embeddings/doc_ids are invalid
        """
        if len(embeddings) != len(doc_ids):
            raise ValueError("Embeddings and doc_ids length mismatch")

        if len(embeddings) < self.min_cluster_size:
            raise ValueError(f"Need at least {self.min_cluster_size} documents")

        loop = asyncio.get_event_loop()

        labels = await loop.run_in_executor(self.executor, self.clusterer.fit_predict, embeddings)

        cluster_info = self.clusterer.get_cluster_info(embeddings, doc_ids)

        if texts:
            cluster_info = self._add_cluster_names(cluster_info, labels, texts)

        result = {
            "algorithm": self.algorithm,
            "n_clusters": self.clusterer.n_clusters_,
            "clusters": cluster_info,
            "labels": labels.tolist(),
        }

        if self.enable_visualization and self.visualizer:
            try:
                projection = await loop.run_in_executor(
                    self.executor, self.visualizer.fit_transform, embeddings
                )

                viz_data = self.visualizer.create_visualization_data(
                    projection, labels, doc_ids, doc_titles
                )

                result["visualization"] = viz_data
            except Exception:
                result["visualization"] = None

        return result

    def _add_cluster_names(
        self, cluster_info: list[dict[str, Any]], labels: np.ndarray, texts: list[str]
    ) -> list[dict[str, Any]]:
        """Generate human-readable cluster names from text.

        Args:
            cluster_info: Cluster information dictionaries
            labels: Cluster labels
            texts: Document texts

        Returns:
            Updated cluster info with names
        """
        from collections import Counter
        import re

        for cluster in cluster_info:
            cluster_id = cluster["cluster_id"]

            if cluster_id == -1:
                cluster["name"] = "Outliers"
                continue

            mask = labels == cluster_id
            cluster_texts = [texts[i] for i in range(len(texts)) if mask[i]]

            words = []
            for text in cluster_texts:
                text_words = re.findall(r"\b\w{4,}\b", text.lower())
                words.extend(text_words[:50])

            word_counts = Counter(words)

            stop_words = {
                "this",
                "that",
                "with",
                "from",
                "have",
                "been",
                "will",
                "would",
                "could",
                "should",
            }
            word_counts = {w: c for w, c in word_counts.items() if w not in stop_words}

            if word_counts:
                top_words = [w for w, _ in word_counts.most_common(3)]
                cluster["name"] = " ".join(top_words).title()
            else:
                cluster["name"] = f"Cluster {cluster_id}"

        return cluster_info

    async def incremental_cluster(
        self, new_embeddings: np.ndarray, new_doc_ids: list[str]
    ) -> list[int]:
        """Assign new documents to existing clusters.

        Args:
            new_embeddings: Embeddings for new documents
            new_doc_ids: IDs for new documents

        Returns:
            Cluster assignments for new documents

        Raises:
            ValueError: If clusterer not fitted
        """
        if self.clusterer.labels_ is None:
            raise ValueError("Clusterer not fitted yet")

        loop = asyncio.get_event_loop()

        if self.algorithm == "agglomerative":
            raise NotImplementedError(
                "Agglomerative clustering doesn't support incremental updates"
            )

        labels = await loop.run_in_executor(self.executor, self.clusterer.predict, new_embeddings)

        return labels.tolist()

    def get_cluster_summary(self, cluster_id: int) -> dict[str, Any] | None:
        """Get summary information for a specific cluster.

        Args:
            cluster_id: Cluster ID

        Returns:
            Cluster summary dictionary or None if not found
        """
        if self.clusterer.labels_ is None:
            raise ValueError("Clusterer not fitted yet")

        mask = self.clusterer.labels_ == cluster_id
        if not np.any(mask):
            return None

        return {"cluster_id": cluster_id, "size": int(np.sum(mask)), "is_outlier": cluster_id == -1}

    def get_similar_clusters(self, cluster_id: int, top_k: int = 5) -> list[tuple[int, float]]:
        """Find clusters similar to a given cluster.

        Args:
            cluster_id: Reference cluster ID
            top_k: Number of similar clusters to return

        Returns:
            List of (cluster_id, similarity_score) tuples
        """
        if self.clusterer.centroids_ is None:
            raise ValueError("Clusterer not fitted yet")

        if cluster_id < 0 or cluster_id >= len(self.clusterer.centroids_):
            raise ValueError("Invalid cluster_id")

        ref_centroid = self.clusterer.centroids_[cluster_id]

        similarities = []
        for i, centroid in enumerate(self.clusterer.centroids_):
            if i != cluster_id:
                sim = np.dot(ref_centroid, centroid) / (
                    np.linalg.norm(ref_centroid) * np.linalg.norm(centroid)
                )
                similarities.append((i, float(sim)))

        similarities.sort(key=lambda x: x[1], reverse=True)
        return similarities[:top_k]
