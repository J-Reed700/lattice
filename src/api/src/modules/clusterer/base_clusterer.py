"""Base clusterer interface and utilities.

This module defines the base interface for all clustering algorithms
and provides common utility functions.
"""

from abc import ABC, abstractmethod
from typing import Any

import numpy as np


class BaseClusterer(ABC):
    """Abstract base class for document clustering algorithms.

    All clusterer implementations should inherit from this class
    and implement the required methods.

    Attributes:
        n_clusters: Number of clusters (if applicable)
        min_cluster_size: Minimum cluster size
        metric: Distance metric ('cosine', 'euclidean', etc.)
    """

    def __init__(
        self, n_clusters: int | None = None, min_cluster_size: int = 5, metric: str = "cosine"
    ):
        """Initialize base clusterer.

        Args:
            n_clusters: Number of clusters (None for auto-detection)
            min_cluster_size: Minimum documents per cluster
            metric: Distance metric to use
        """
        self.n_clusters = n_clusters
        self.min_cluster_size = min_cluster_size
        self.metric = metric

        self.labels_ = None
        self.centroids_ = None
        self.n_clusters_ = None

    @abstractmethod
    def fit(self, embeddings: np.ndarray) -> "BaseClusterer":
        """Fit the clustering model on embeddings.

        Args:
            embeddings: Array of shape (n_documents, embedding_dim)

        Returns:
            Self for method chaining

        Raises:
            ValueError: If embeddings are invalid
        """

    @abstractmethod
    def predict(self, embeddings: np.ndarray) -> np.ndarray:
        """Predict cluster assignments for new embeddings.

        Args:
            embeddings: Array of shape (n_documents, embedding_dim)

        Returns:
            Array of cluster labels (shape: n_documents)
        """

    def fit_predict(self, embeddings: np.ndarray) -> np.ndarray:
        """Fit model and return cluster assignments.

        Args:
            embeddings: Array of shape (n_documents, embedding_dim)

        Returns:
            Array of cluster labels (shape: n_documents)
        """
        self.fit(embeddings)
        return self.labels_

    def get_cluster_info(
        self, embeddings: np.ndarray, doc_ids: list[str] | None = None
    ) -> list[dict[str, Any]]:
        """Get information about each cluster.

        Args:
            embeddings: Document embeddings
            doc_ids: Optional list of document IDs

        Returns:
            List of cluster info dictionaries

        Example:
            >>> info = clusterer.get_cluster_info(embeddings, doc_ids)
            >>> print(info[0])
            {
                "cluster_id": 0,
                "size": 42,
                "density": 0.75,
                "doc_ids": ["id1", "id2", ...],
                "centroid": [0.1, 0.2, ...]
            }
        """
        if self.labels_ is None:
            raise ValueError("Model not fitted yet")

        doc_ids = doc_ids or [str(i) for i in range(len(embeddings))]

        clusters_info = []
        unique_labels = np.unique(self.labels_)

        for label in unique_labels:
            mask = self.labels_ == label
            cluster_embeddings = embeddings[mask]
            cluster_doc_ids = [doc_ids[i] for i in range(len(doc_ids)) if mask[i]]

            centroid = np.mean(cluster_embeddings, axis=0)

            distances = self._compute_distances(cluster_embeddings, centroid)
            density = 1.0 / (1.0 + np.mean(distances)) if len(distances) > 0 else 0.0

            clusters_info.append(
                {
                    "cluster_id": int(label),
                    "size": int(np.sum(mask)),
                    "density": float(density),
                    "doc_ids": cluster_doc_ids,
                    "centroid": centroid.tolist() if isinstance(centroid, np.ndarray) else centroid,
                    "is_outlier": label == -1,
                }
            )

        return clusters_info

    def _compute_distances(self, embeddings: np.ndarray, centroid: np.ndarray) -> np.ndarray:
        """Compute distances from embeddings to centroid.

        Args:
            embeddings: Array of embeddings
            centroid: Cluster centroid

        Returns:
            Array of distances
        """
        if self.metric == "cosine":
            similarities = np.dot(embeddings, centroid) / (
                np.linalg.norm(embeddings, axis=1) * np.linalg.norm(centroid)
            )
            return 1.0 - similarities
        if self.metric == "euclidean":
            return np.linalg.norm(embeddings - centroid, axis=1)
        return np.linalg.norm(embeddings - centroid, axis=1)

    def get_cluster_membership_scores(self, embeddings: np.ndarray) -> np.ndarray:
        """Get membership confidence scores for each document.

        Args:
            embeddings: Document embeddings

        Returns:
            Array of membership scores (shape: n_documents)
        """
        if self.labels_ is None or self.centroids_ is None:
            raise ValueError("Model not fitted yet")

        scores = np.zeros(len(embeddings))

        for i, (embedding, label) in enumerate(zip(embeddings, self.labels_, strict=False)):
            if label == -1:
                scores[i] = 0.0
            else:
                centroid = self.centroids_[label]
                distance = self._compute_distances(embedding.reshape(1, -1), centroid)[0]

                score = 1.0 / (1.0 + distance)
                scores[i] = score

        return scores

    def get_outliers(self, doc_ids: list[str] | None = None) -> list[str]:
        """Get list of outlier document IDs.

        Args:
            doc_ids: Optional list of document IDs

        Returns:
            List of outlier document IDs
        """
        if self.labels_ is None:
            raise ValueError("Model not fitted yet")

        doc_ids = doc_ids or [str(i) for i in range(len(self.labels_))]

        outlier_indices = np.where(self.labels_ == -1)[0]
        return [doc_ids[i] for i in outlier_indices]

    @staticmethod
    def normalize_embeddings(embeddings: np.ndarray) -> np.ndarray:
        """Normalize embeddings to unit length.

        Args:
            embeddings: Array of embeddings

        Returns:
            Normalized embeddings
        """
        norms = np.linalg.norm(embeddings, axis=1, keepdims=True)
        norms = np.where(norms == 0, 1, norms)
        return embeddings / norms
