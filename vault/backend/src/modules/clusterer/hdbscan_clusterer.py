"""HDBSCAN-based document clustering.

HDBSCAN (Hierarchical Density-Based Spatial Clustering of Applications with Noise)
is excellent for finding clusters of varying density and identifying outliers.

Contract:
    Input: Document embeddings (numpy array)
    Output: Cluster labels with outlier detection (-1 for outliers)
    Performance: ~1-2 seconds for 1000 documents
"""


import numpy as np

try:
    import hdbscan

    HDBSCAN_AVAILABLE = True
except ImportError:
    HDBSCAN_AVAILABLE = False

from .base_clusterer import BaseClusterer


class HDBSCANClusterer(BaseClusterer):
    """HDBSCAN-based document clustering with outlier detection.

    HDBSCAN is density-based and automatically determines the number
    of clusters. It handles outliers well and works with varying densities.

    Attributes:
        min_cluster_size: Minimum documents per cluster
        min_samples: Minimum samples in neighborhood
        metric: Distance metric
        cluster_selection_method: Method for selecting clusters

    Example:
        >>> clusterer = HDBSCANClusterer(min_cluster_size=10)
        >>> labels = clusterer.fit_predict(embeddings)
        >>> outliers = clusterer.get_outliers(doc_ids)
    """

    def __init__(
        self,
        min_cluster_size: int = 5,
        min_samples: int | None = None,
        metric: str = "euclidean",
        cluster_selection_method: str = "eom",
    ):
        """Initialize HDBSCAN clusterer.

        Args:
            min_cluster_size: Minimum documents per cluster
            min_samples: Minimum samples in neighborhood (None = min_cluster_size)
            metric: Distance metric ('euclidean', 'cosine', 'manhattan')
            cluster_selection_method: 'eom' (default) or 'leaf'
        """
        super().__init__(None, min_cluster_size, metric)

        self.min_samples = min_samples or min_cluster_size
        self.cluster_selection_method = cluster_selection_method

        if not HDBSCAN_AVAILABLE:
            raise ImportError("hdbscan not installed. Install with: pip install hdbscan")

        self.model = hdbscan.HDBSCAN(
            min_cluster_size=self.min_cluster_size,
            min_samples=self.min_samples,
            metric=self.metric,
            cluster_selection_method=self.cluster_selection_method,
        )

    def fit(self, embeddings: np.ndarray) -> "HDBSCANClusterer":
        """Fit HDBSCAN model on embeddings.

        Args:
            embeddings: Array of shape (n_documents, embedding_dim)

        Returns:
            Self for method chaining
        """
        if embeddings.shape[0] < self.min_cluster_size:
            raise ValueError(f"Need at least {self.min_cluster_size} documents")

        if self.metric == "cosine":
            embeddings = self.normalize_embeddings(embeddings)

        self.model.fit(embeddings)

        self.labels_ = self.model.labels_
        self.n_clusters_ = len(set(self.labels_)) - (1 if -1 in self.labels_ else 0)

        self._compute_centroids(embeddings)

        return self

    def predict(self, embeddings: np.ndarray) -> np.ndarray:
        """Predict cluster assignments for new embeddings.

        Args:
            embeddings: Array of shape (n_documents, embedding_dim)

        Returns:
            Array of cluster labels
        """
        if self.model is None:
            raise ValueError("Model not fitted yet")

        if self.metric == "cosine":
            embeddings = self.normalize_embeddings(embeddings)

        labels, strengths = hdbscan.approximate_predict(self.model, embeddings)
        return labels

    def _compute_centroids(self, embeddings: np.ndarray) -> None:
        """Compute cluster centroids.

        Args:
            embeddings: Document embeddings
        """
        unique_labels = np.unique(self.labels_)
        unique_labels = unique_labels[unique_labels != -1]

        self.centroids_ = np.zeros((len(unique_labels), embeddings.shape[1]))

        for i, label in enumerate(unique_labels):
            mask = self.labels_ == label
            self.centroids_[label] = np.mean(embeddings[mask], axis=0)

    def get_cluster_persistence(self) -> dict:
        """Get cluster persistence scores.

        Returns:
            Dictionary mapping cluster IDs to persistence scores
        """
        if self.model is None:
            raise ValueError("Model not fitted yet")

        return {
            int(label): float(score)
            for label, score in enumerate(self.model.cluster_persistence_)
            if label != -1
        }
