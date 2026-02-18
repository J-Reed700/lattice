"""Keyword extraction using multiple algorithms.

This module provides keyword extraction using:
- TF-IDF (sklearn)
- RAKE (Rapid Automatic Keyword Extraction)
- KeyBERT (embedding-based keyword extraction)

Contract:
    Input: Text string
    Output: List of (keyword, score) tuples
    Performance: < 200ms for 1000 words
"""

from collections import Counter
import re

try:
    from sklearn.feature_extraction.text import TfidfVectorizer

    SKLEARN_AVAILABLE = True
except ImportError:
    SKLEARN_AVAILABLE = False

try:
    from keybert import KeyBERT

    KEYBERT_AVAILABLE = True
except ImportError:
    KEYBERT_AVAILABLE = False


class KeywordExtractor:
    """Extract keywords from text using multiple algorithms.

    Supports TF-IDF, RAKE, and KeyBERT (if available).
    Falls back to simpler methods if advanced libraries are not installed.

    Attributes:
        method: Extraction method ('tfidf', 'rake', 'keybert', 'auto')
        top_k: Number of keywords to extract
        min_ngram: Minimum n-gram size
        max_ngram: Maximum n-gram size

    Example:
        >>> extractor = KeywordExtractor(method='rake', top_k=10)
        >>> keywords = extractor.extract("Machine learning is transforming...")
        >>> print(keywords)
        [("machine learning", 0.92), ("transforming industries", 0.85), ...]
    """

    def __init__(
        self,
        method: str = "auto",
        top_k: int = 10,
        min_ngram: int = 1,
        max_ngram: int = 3,
        stop_words: list[str] | None = None,
    ):
        """Initialize keyword extractor.

        Args:
            method: Extraction method ('tfidf', 'rake', 'keybert', 'auto')
            top_k: Number of keywords to extract
            min_ngram: Minimum n-gram size
            max_ngram: Maximum n-gram size
            stop_words: Custom stop words list
        """
        self.method = method
        self.top_k = top_k
        self.min_ngram = min_ngram
        self.max_ngram = max_ngram
        self.stop_words = stop_words or self._default_stop_words()

        self.keybert_model = None
        if method == "keybert" and KEYBERT_AVAILABLE:
            try:
                self.keybert_model = KeyBERT()
            except Exception:
                pass

    def extract(self, text: str) -> list[tuple[str, float]]:
        """Extract keywords from text.

        Args:
            text: Input text

        Returns:
            List of (keyword, confidence_score) tuples, sorted by score

        Raises:
            ValueError: If text is empty or invalid
        """
        if not text or not text.strip():
            raise ValueError("Text cannot be empty")

        text = self._preprocess(text)

        if self.method == "auto":
            method = self._choose_method()
        else:
            method = self.method

        if method == "keybert" and self.keybert_model:
            return self._extract_keybert(text)
        if method == "tfidf" and SKLEARN_AVAILABLE:
            return self._extract_tfidf(text)
        return self._extract_rake(text)

    def _choose_method(self) -> str:
        """Choose best available method."""
        if KEYBERT_AVAILABLE:
            return "keybert"
        if SKLEARN_AVAILABLE:
            return "tfidf"
        return "rake"

    def _preprocess(self, text: str) -> str:
        """Preprocess text."""
        text = re.sub(r"\s+", " ", text)
        text = text.strip()
        return text

    def _extract_keybert(self, text: str) -> list[tuple[str, float]]:
        """Extract keywords using KeyBERT.

        Args:
            text: Preprocessed text

        Returns:
            List of (keyword, score) tuples
        """
        try:
            keywords = self.keybert_model.extract_keywords(
                text,
                keyphrase_ngram_range=(self.min_ngram, self.max_ngram),
                stop_words="english",
                top_n=self.top_k,
                use_mmr=True,
                diversity=0.5,
            )
            return [(kw, float(score)) for kw, score in keywords]
        except Exception:
            return self._extract_rake(text)

    def _extract_tfidf(self, text: str) -> list[tuple[str, float]]:
        """Extract keywords using TF-IDF.

        Args:
            text: Preprocessed text

        Returns:
            List of (keyword, score) tuples
        """
        try:
            vectorizer = TfidfVectorizer(
                max_features=self.top_k * 2,
                ngram_range=(self.min_ngram, self.max_ngram),
                stop_words="english",
            )

            tfidf_matrix = vectorizer.fit_transform([text])
            feature_names = vectorizer.get_feature_names_out()

            scores = tfidf_matrix.toarray()[0]
            keywords = [
                (feature_names[i], float(scores[i]))
                for i in scores.argsort()[::-1][: self.top_k]
                if scores[i] > 0
            ]

            return keywords
        except Exception:
            return self._extract_rake(text)

    def _extract_rake(self, text: str) -> list[tuple[str, float]]:
        """Extract keywords using RAKE algorithm.

        RAKE (Rapid Automatic Keyword Extraction) scores phrases based on
        word frequency and co-occurrence patterns.

        Args:
            text: Preprocessed text

        Returns:
            List of (keyword, score) tuples
        """
        sentences = re.split(r"[.!?;]", text)

        phrases = []
        for sentence in sentences:
            words = re.findall(r"\b\w+\b", sentence.lower())

            current_phrase = []
            for word in words:
                if word not in self.stop_words and len(word) > 2:
                    current_phrase.append(word)
                else:
                    if current_phrase and len(current_phrase) <= self.max_ngram:
                        phrases.append(" ".join(current_phrase))
                    current_phrase = []

            if current_phrase and len(current_phrase) <= self.max_ngram:
                phrases.append(" ".join(current_phrase))

        word_freq = Counter()
        word_degree = Counter()

        for phrase in phrases:
            words = phrase.split()
            degree = len(words) - 1
            for word in words:
                word_freq[word] += 1
                word_degree[word] += degree

        word_scores = {
            word: word_degree[word] / word_freq[word] if word_freq[word] > 0 else 0
            for word in word_freq
        }

        phrase_scores = {}
        for phrase in phrases:
            words = phrase.split()
            if len(words) >= self.min_ngram:
                score = sum(word_scores.get(word, 0) for word in words)
                phrase_scores[phrase] = score

        max_score = max(phrase_scores.values()) if phrase_scores else 1.0
        normalized_scores = [(phrase, score / max_score) for phrase, score in phrase_scores.items()]

        normalized_scores.sort(key=lambda x: x[1], reverse=True)
        return normalized_scores[: self.top_k]

    def _default_stop_words(self) -> set:
        """Default English stop words."""
        return {
            "a",
            "an",
            "and",
            "are",
            "as",
            "at",
            "be",
            "by",
            "for",
            "from",
            "has",
            "he",
            "in",
            "is",
            "it",
            "its",
            "of",
            "on",
            "that",
            "the",
            "to",
            "was",
            "will",
            "with",
            "this",
            "but",
            "they",
            "have",
            "had",
            "what",
            "when",
            "where",
            "who",
            "which",
            "why",
            "how",
        }
