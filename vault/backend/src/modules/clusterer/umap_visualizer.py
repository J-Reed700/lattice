"""UMAP-based cluster visualization.

UMAP (Uniform Manifold Approximation and Projection) reduces high-dimensional
embeddings to 2D or 3D for interactive visualization.

Contract:
    Input: High-dimensional embeddings
    Output: 2D/3D projections
    Performance: ~1-2 seconds for 1000 documents
"""

from typing import Any

import numpy as np

try:
    import umap

    UMAP_AVAILABLE = True
except ImportError:
    UMAP_AVAILABLE = False


class UMAPVisualizer:
    """UMAP-based dimensionality reduction for cluster visualization.

    Reduces high-dimensional embeddings to 2D or 3D for visualization
    while preserving local and global structure.

    Attributes:
        n_components: Number of dimensions (2 or 3)
        n_neighbors: Number of neighbors for manifold approximation
        min_dist: Minimum distance between points
        metric: Distance metric

    Example:
        >>> visualizer = UMAPVisualizer(n_components=2)
        >>> coords = visualizer.fit_transform(embeddings)
        >>> viz_data = visualizer.create_visualization_data(coords, labels, doc_ids)
    """

    def __init__(
        self,
        n_components: int = 2,
        n_neighbors: int = 15,
        min_dist: float = 0.1,
        metric: str = "cosine",
    ):
        """Initialize UMAP visualizer.

        Args:
            n_components: Number of dimensions (2 or 3)
            n_neighbors: Number of neighbors
            min_dist: Minimum distance between points
            metric: Distance metric
        """
        if n_components not in [2, 3]:
            raise ValueError("n_components must be 2 or 3")

        self.n_components = n_components
        self.n_neighbors = n_neighbors
        self.min_dist = min_dist
        self.metric = metric

        if not UMAP_AVAILABLE:
            raise ImportError("umap-learn not installed. Install with: pip install umap-learn")

        self.model = umap.UMAP(
            n_components=n_components,
            n_neighbors=n_neighbors,
            min_dist=min_dist,
            metric=metric,
            random_state=42,
        )

    def fit_transform(self, embeddings: np.ndarray) -> np.ndarray:
        """Reduce embeddings to 2D or 3D.

        Args:
            embeddings: High-dimensional embeddings

        Returns:
            Array of shape (n_documents, n_components)
        """
        if embeddings.shape[0] < self.n_neighbors:
            raise ValueError(f"Need at least {self.n_neighbors} documents")

        projection = self.model.fit_transform(embeddings)
        return projection

    def transform(self, embeddings: np.ndarray) -> np.ndarray:
        """Transform new embeddings using fitted model.

        Args:
            embeddings: High-dimensional embeddings

        Returns:
            Array of shape (n_documents, n_components)
        """
        if self.model is None:
            raise ValueError("Model not fitted yet")

        return self.model.transform(embeddings)

    def create_visualization_data(
        self,
        projection: np.ndarray,
        labels: np.ndarray,
        doc_ids: list[str],
        doc_titles: list[str] | None = None,
    ) -> dict[str, Any]:
        """Create visualization-ready data structure.

        Args:
            projection: 2D or 3D coordinates
            labels: Cluster labels
            doc_ids: Document IDs
            doc_titles: Optional document titles

        Returns:
            Dictionary with visualization data

        Example:
            >>> viz_data = {
            ...     "points": [
            ...         {"id": "doc1", "x": 0.1, "y": 0.2, "cluster": 0, "title": "..."},
            ...         ...
            ...     ],
            ...     "clusters": {
            ...         "0": {"centroid": [0.15, 0.25], "size": 42, "color": "#FF5733"},
            ...         ...
            ...     }
            ... }
        """
        doc_titles = doc_titles or [f"Document {i}" for i in range(len(doc_ids))]

        points = []
        for i in range(len(projection)):
            point = {
                "id": doc_ids[i],
                "title": doc_titles[i],
                "cluster": int(labels[i]),
                "x": float(projection[i][0]),
                "y": float(projection[i][1]),
            }

            if self.n_components == 3:
                point["z"] = float(projection[i][2])

            points.append(point)

        clusters = {}
        unique_labels = np.unique(labels)

        colors = self._generate_colors(len(unique_labels))

        for idx, label in enumerate(unique_labels):
            mask = labels == label
            cluster_points = projection[mask]

            centroid = np.mean(cluster_points, axis=0)

            clusters[str(label)] = {
                "id": int(label),
                "size": int(np.sum(mask)),
                "centroid": centroid.tolist(),
                "color": colors[idx],
                "is_outlier": label == -1,
            }

        return {"points": points, "clusters": clusters, "dimensions": self.n_components}

    def _generate_colors(self, n_colors: int) -> list[str]:
        """Generate distinct colors for clusters.

        Args:
            n_colors: Number of colors needed

        Returns:
            List of hex color strings
        """
        colors = [
            "#FF6B6B",
            "#4ECDC4",
            "#45B7D1",
            "#FFA07A",
            "#98D8C8",
            "#F7DC6F",
            "#BB8FCE",
            "#85C1E2",
            "#F8B500",
            "#52B788",
            "#FF8FA3",
            "#6C5CE7",
            "#00B4D8",
            "#FF6348",
            "#26DE81",
        ]

        while len(colors) < n_colors:
            import random

            colors.append(f"#{random.randint(0, 0xFFFFFF):06x}")

        return colors[:n_colors]

    def save_projection(self, filename: str, projection: np.ndarray) -> None:
        """Save projection to file.

        Args:
            filename: Output filename
            projection: Projection coordinates
        """
        np.save(filename, projection)

    def load_projection(self, filename: str) -> np.ndarray:
        """Load projection from file.

        Args:
            filename: Input filename

        Returns:
            Projection coordinates
        """
        return np.load(filename)
