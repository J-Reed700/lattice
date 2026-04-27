"""Agglomerative hierarchical clustering.

Agglomerative clustering builds a hierarchy of clusters bottom-up,
enabling hierarchical document organization.

Contract:
    Input: Document embeddings
    Output: Hierarchical cluster structure
    Performance: ~2-3 seconds for 1000 documents
"""


import numpy as np

try:
    from sklearn.cluster import AgglomerativeClustering

    SKLEARN_AVAILABLE = True
except ImportError:
    SKLEARN_AVAILABLE = False

from .base_clusterer import BaseClusterer


class AgglomerativeClusterer(BaseClusterer):
    """Agglomerative hierarchical clustering.

    Builds clusters hierarchically from bottom-up, supporting
    hierarchical document organization.

    Attributes:
        n_clusters: Number of clusters
        linkage: Linkage criterion ('ward', 'complete', 'average', 'single')
        distance_threshold: Distance threshold for automatic cluster detection

    Example:
        >>> clusterer = AgglomerativeClusterer(n_clusters=10)
        >>> labels = clusterer.fit_predict(embeddings)
        >>> hierarchy = clusterer.get_hierarchy()
    """

    def __init__(
        self,
        n_clusters: int | None = 10,
        linkage: str = "ward",
        distance_threshold: float | None = None,
    ):
        """Initialize Agglomerative clusterer.

        Args:
            n_clusters: Number of clusters (None for auto with distance_threshold)
            linkage: Linkage criterion
            distance_threshold: Distance threshold for auto cluster detection
        """
        super().__init__(n_clusters, min_cluster_size=1, metric="euclidean")

        if not SKLEARN_AVAILABLE:
            raise ImportError("sklearn not installed. Install with: pip install scikit-learn")

        self.linkage = linkage
        self.distance_threshold = distance_threshold

        if distance_threshold is not None:
            n_clusters = None

        self.model = AgglomerativeClustering(
            n_clusters=n_clusters, linkage=linkage, distance_threshold=distance_threshold
        )

    def fit(self, embeddings: np.ndarray) -> "AgglomerativeClusterer":
        """Fit Agglomerative model on embeddings.

        Args:
            embeddings: Array of shape (n_documents, embedding_dim)

        Returns:
            Self for method chaining
        """
        if self.n_clusters and embeddings.shape[0] < self.n_clusters:
            raise ValueError(f"Need at least {self.n_clusters} documents")

        embeddings = self.normalize_embeddings(embeddings)

        self.model.fit(embeddings)

        self.labels_ = self.model.labels_
        self.n_clusters_ = self.model.n_clusters_

        self._compute_centroids(embeddings)

        return self

    def predict(self, embeddings: np.ndarray) -> np.ndarray:
        """Predict cluster assignments (not supported for Agglomerative).

        Agglomerative clustering doesn't support prediction on new data.
        Use fit_predict instead.
        """
        raise NotImplementedError(
            "Agglomerative clustering doesn't support prediction. Use fit_predict instead."
        )

    def _compute_centroids(self, embeddings: np.ndarray) -> None:
        """Compute cluster centroids.

        Args:
            embeddings: Document embeddings
        """
        unique_labels = np.unique(self.labels_)

        self.centroids_ = np.zeros((len(unique_labels), embeddings.shape[1]))

        for label in unique_labels:
            mask = self.labels_ == label
            self.centroids_[label] = np.mean(embeddings[mask], axis=0)

    def get_hierarchy(self) -> list[tuple[int, int]]:
        """Get hierarchical cluster structure.

        Returns:
            List of (parent_cluster, child_cluster) tuples
        """
        if self.model is None or not hasattr(self.model, "children_"):
            raise ValueError("Model not fitted yet or doesn't support hierarchy")

        hierarchy = []
        for i, children in enumerate(self.model.children_):
            parent = i + len(self.model.children_)
            hierarchy.append((parent, int(children[0])))
            hierarchy.append((parent, int(children[1])))

        return hierarchy
