"""Document clustering module for topic-based organization.

This module provides automatic document clustering using multiple algorithms:
- HDBSCAN (density-based, handles outliers)
- K-Means (fast, scalable)
- Agglomerative (hierarchical structure)
- UMAP (dimensionality reduction for visualization)

Public Interface:
    - DocumentClusterer: Main class for document clustering
    - HDBSCANClusterer: HDBSCAN-based clustering
    - KMeansClusterer: K-Means clustering
    - AgglomerativeClusterer: Hierarchical clustering
    - UMAPVisualizer: 2D/3D cluster visualization

Example:
    >>> from modules.clusterer import DocumentClusterer
    >>> clusterer = DocumentClusterer(algorithm='hdbscan')
    >>> clusters = await clusterer.cluster_documents(embeddings, doc_ids)
    >>> print(clusters)
    [
        {"cluster_id": 0, "name": "Machine Learning", "doc_ids": [...], "size": 42},
        {"cluster_id": 1, "name": "Data Science", "doc_ids": [...], "size": 35}
    ]
"""

from .agglomerative_clusterer import AgglomerativeClusterer
from .clusterer import DocumentClusterer
from .hdbscan_clusterer import HDBSCANClusterer
from .kmeans_clusterer import KMeansClusterer
from .umap_visualizer import UMAPVisualizer

__all__ = [
    "AgglomerativeClusterer",
    "DocumentClusterer",
    "HDBSCANClusterer",
    "KMeansClusterer",
    "UMAPVisualizer",
]
