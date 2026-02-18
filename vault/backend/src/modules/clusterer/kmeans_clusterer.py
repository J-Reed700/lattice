"""K-Means document clustering.

K-Means is fast, scalable, and works well when the number of clusters is known.

Contract:
    Input: Document embeddings + number of clusters
    Output: Cluster labels and centroids
    Performance: < 500ms for 1000 documents
"""

import numpy as np

try:
    from sklearn.cluster import KMeans, MiniBatchKMeans

    SKLEARN_AVAILABLE = True
except ImportError:
    SKLEARN_AVAILABLE = False

from .base_clusterer import BaseClusterer


class KMeansClusterer(BaseClusterer):
    """K-Means document clustering.

    Fast and scalable clustering when the number of clusters is known.
    Uses Mini-Batch K-Means for large datasets.

    Attributes:
        n_clusters: Number of clusters
        max_iter: Maximum iterations
        n_init: Number of initializations
        use_minibatch: Use Mini-Batch K-Means for large datasets

    Example:
        >>> clusterer = KMeansClusterer(n_clusters=10)
        >>> labels = clusterer.fit_predict(embeddings)
        >>> centroids = clusterer.centroids_
    """

    def __init__(
        self,
        n_clusters: int = 10,
        max_iter: int = 300,
        n_init: int = 10,
        use_minibatch: bool = False,
        batch_size: int = 1000,
    ):
        """Initialize K-Means clusterer.

        Args:
            n_clusters: Number of clusters
            max_iter: Maximum iterations
            n_init: Number of initializations
            use_minibatch: Use Mini-Batch K-Means
            batch_size: Batch size for Mini-Batch K-Means
        """
        super().__init__(n_clusters, min_cluster_size=1, metric="euclidean")

        if not SKLEARN_AVAILABLE:
            raise ImportError("sklearn not installed. Install with: pip install scikit-learn")

        self.max_iter = max_iter
        self.n_init = n_init
        self.use_minibatch = use_minibatch
        self.batch_size = batch_size

        if use_minibatch:
            self.model = MiniBatchKMeans(
                n_clusters=n_clusters,
                max_iter=max_iter,
                n_init=n_init,
                batch_size=batch_size,
                random_state=42,
            )
        else:
            self.model = KMeans(
                n_clusters=n_clusters, max_iter=max_iter, n_init=n_init, random_state=42
            )

    def fit(self, embeddings: np.ndarray) -> "KMeansClusterer":
        """Fit K-Means model on embeddings.

        Args:
            embeddings: Array of shape (n_documents, embedding_dim)

        Returns:
            Self for method chaining
        """
        if embeddings.shape[0] < self.n_clusters:
            raise ValueError(
                f"Need at least {self.n_clusters} documents for {self.n_clusters} clusters"
            )

        embeddings = self.normalize_embeddings(embeddings)

        self.model.fit(embeddings)

        self.labels_ = self.model.labels_
        self.centroids_ = self.model.cluster_centers_
        self.n_clusters_ = self.n_clusters

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

        embeddings = self.normalize_embeddings(embeddings)

        return self.model.predict(embeddings)

    def get_inertia(self) -> float:
        """Get sum of squared distances to nearest cluster center.

        Returns:
            Inertia score (lower is better)
        """
        if self.model is None:
            raise ValueError("Model not fitted yet")

        return float(self.model.inertia_)
