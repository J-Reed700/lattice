"""Topic modeling for document categorization.

This module generates topic labels for documents using LDA or simple
clustering-based approaches.

Contract:
    Input: Text string or list of texts
    Output: List of (topic_label, confidence) tuples
    Performance: < 1 second for single document
"""

from collections import Counter
import re

try:
    from sklearn.decomposition import LatentDirichletAllocation
    from sklearn.feature_extraction.text import CountVectorizer

    SKLEARN_AVAILABLE = True
except ImportError:
    SKLEARN_AVAILABLE = False


class TopicModeler:
    """Generate topic labels for documents.

    Uses LDA (Latent Dirichlet Allocation) if sklearn is available,
    falls back to TF-IDF based topic inference.

    Attributes:
        n_topics: Number of topics to model
        n_top_words: Number of top words per topic
        min_df: Minimum document frequency
        max_df: Maximum document frequency

    Example:
        >>> modeler = TopicModeler(n_topics=10)
        >>> topics = modeler.extract_topics("Machine learning...")
        >>> print(topics)
        [("machine learning and ai", 0.85), ("data science", 0.62)]
    """

    def __init__(
        self, n_topics: int = 10, n_top_words: int = 5, min_df: int = 2, max_df: float = 0.8
    ):
        """Initialize topic modeler.

        Args:
            n_topics: Number of topics to model
            n_top_words: Number of top words per topic
            min_df: Minimum document frequency
            max_df: Maximum document frequency
        """
        self.n_topics = n_topics
        self.n_top_words = n_top_words
        self.min_df = min_df
        self.max_df = max_df

        self.lda_model = None
        self.vectorizer = None
        self.topic_labels = {}

    def fit(self, documents: list[str]) -> None:
        """Fit topic model on a corpus of documents.

        Args:
            documents: List of document texts

        Raises:
            ValueError: If documents list is empty
        """
        if not documents:
            raise ValueError("Documents list cannot be empty")

        if SKLEARN_AVAILABLE:
            self._fit_lda(documents)
        else:
            self._fit_simple(documents)

    def _fit_lda(self, documents: list[str]) -> None:
        """Fit LDA topic model.

        Args:
            documents: List of document texts
        """
        self.vectorizer = CountVectorizer(
            max_df=self.max_df, min_df=self.min_df, stop_words="english", lowercase=True
        )

        doc_term_matrix = self.vectorizer.fit_transform(documents)

        self.lda_model = LatentDirichletAllocation(
            n_components=self.n_topics, max_iter=10, random_state=42
        )

        self.lda_model.fit(doc_term_matrix)

        feature_names = self.vectorizer.get_feature_names_out()
        for topic_idx, topic in enumerate(self.lda_model.components_):
            top_indices = topic.argsort()[-self.n_top_words :][::-1]
            top_words = [feature_names[i] for i in top_indices]
            label = " ".join(top_words[:3])
            self.topic_labels[topic_idx] = label

    def _fit_simple(self, documents: list[str]) -> None:
        """Fit simple frequency-based topic model (fallback).

        Args:
            documents: List of document texts
        """
        all_words = []
        for doc in documents:
            words = re.findall(r"\b\w{4,}\b", doc.lower())
            all_words.extend(words)

        word_counts = Counter(all_words)
        common_words = word_counts.most_common(100)

        topics_per_group = max(1, len(common_words) // self.n_topics)
        for i in range(self.n_topics):
            start = i * topics_per_group
            end = start + topics_per_group
            topic_words = [word for word, _ in common_words[start:end][:3]]
            self.topic_labels[i] = " ".join(topic_words)

    def extract_topics(self, text: str, top_n: int = 3) -> list[tuple[str, float]]:
        """Extract top topics for a single document.

        Args:
            text: Input text
            top_n: Number of top topics to return

        Returns:
            List of (topic_label, confidence) tuples

        Raises:
            ValueError: If model not fitted or text is empty
        """
        if not text or not text.strip():
            raise ValueError("Text cannot be empty")

        if not self.topic_labels:
            return self._extract_simple_topics(text, top_n)

        if self.lda_model and self.vectorizer:
            return self._extract_lda_topics(text, top_n)
        return self._extract_simple_topics(text, top_n)

    def _extract_lda_topics(self, text: str, top_n: int) -> list[tuple[str, float]]:
        """Extract topics using fitted LDA model.

        Args:
            text: Input text
            top_n: Number of topics to return

        Returns:
            List of (topic_label, confidence) tuples
        """
        doc_vector = self.vectorizer.transform([text])

        topic_dist = self.lda_model.transform(doc_vector)[0]

        top_indices = topic_dist.argsort()[-top_n:][::-1]

        topics = [
            (self.topic_labels[idx], float(topic_dist[idx]))
            for idx in top_indices
            if topic_dist[idx] > 0.1
        ]

        return topics

    def _extract_simple_topics(self, text: str, top_n: int) -> list[tuple[str, float]]:
        """Extract topics using simple word frequency (fallback).

        Args:
            text: Input text
            top_n: Number of topics to return

        Returns:
            List of (topic_label, confidence) tuples
        """
        words = re.findall(r"\b\w{4,}\b", text.lower())
        word_counts = Counter(words)

        total = sum(word_counts.values())
        if total == 0:
            return []

        common = word_counts.most_common(top_n * 3)

        topics = []
        for i in range(0, min(len(common), top_n * 3), 3):
            topic_words = [word for word, _ in common[i : i + 3]]
            if len(topic_words) >= 2:
                label = " ".join(topic_words)
                avg_count = sum(word_counts[w] for w in topic_words) / len(topic_words)
                confidence = min(1.0, avg_count / total * 10)
                topics.append((label, confidence))

        return topics[:top_n]

    def get_topic_distribution(self, text: str) -> dict[str, float]:
        """Get full topic distribution for a document.

        Args:
            text: Input text

        Returns:
            Dictionary mapping topic labels to probabilities

        Example:
            >>> dist = modeler.get_topic_distribution(text)
            >>> print(dist)
            {
                "machine learning ai": 0.45,
                "data science analytics": 0.30,
                "software development": 0.15,
                ...
            }
        """
        topics = self.extract_topics(text, top_n=self.n_topics)
        return {label: conf for label, conf in topics}

    def infer_document_category(
        self, text: str, categories: dict[str, list[str]]
    ) -> tuple[str, float]:
        """Infer document category based on topic keywords.

        Args:
            text: Input text
            categories: Dict mapping category names to keyword lists

        Returns:
            Tuple of (category_name, confidence)

        Example:
            >>> categories = {
            ...     "Technology": ["ai", "software", "computer"],
            ...     "Science": ["research", "study", "experiment"]
            ... }
            >>> category, conf = modeler.infer_document_category(text, categories)
        """
        topics = self.extract_topics(text, top_n=5)

        if not topics:
            return ("unknown", 0.0)

        topic_text = " ".join(label for label, _ in topics)
        topic_words = set(topic_text.lower().split())

        scores = {}
        for category, keywords in categories.items():
            keyword_set = set(kw.lower() for kw in keywords)
            overlap = len(topic_words & keyword_set)
            scores[category] = overlap / len(keyword_set) if keyword_set else 0

        if not scores:
            return ("unknown", 0.0)

        best_category = max(scores, key=scores.get)
        confidence = scores[best_category]

        return (best_category, confidence)
