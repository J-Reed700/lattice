"""Main auto-tagging orchestrator.

This module combines keyword extraction, NER, topic modeling, and
rule-based tagging into a unified auto-tagging system.

Contract:
    Input: Document text + metadata
    Output: Comprehensive tag list with sources and confidence scores
    Performance: < 1 second for typical document
"""

import asyncio
from concurrent.futures import ThreadPoolExecutor
from typing import Any

from .entity_extractor import EntityExtractor
from .keyword_extractor import KeywordExtractor
from .rule_based import RuleBasedTagger, TagRule
from .topic_modeler import TopicModeler


class AutoTagger:
    """Unified auto-tagging system combining multiple extraction methods.

    Orchestrates keyword extraction, NER, topic modeling, and rule-based
    tagging to generate comprehensive document tags.

    Attributes:
        keyword_extractor: KeywordExtractor instance
        entity_extractor: EntityExtractor instance
        topic_modeler: TopicModeler instance
        rule_tagger: RuleBasedTagger instance
        min_confidence: Minimum confidence threshold for tags
        max_tags_per_source: Maximum tags per extraction method

    Example:
        >>> tagger = AutoTagger()
        >>> tags = await tagger.tag_document(
        ...     text="Machine learning is transforming...",
        ...     metadata={"filename": "ml_intro.pdf"}
        ... )
        >>> print(tags)
        [
            {
                "name": "machine learning",
                "source": "keyword",
                "confidence": 0.92,
                "entity_type": None,
                "context": "...machine learning is..."
            },
            ...
        ]
    """

    def __init__(
        self,
        keyword_method: str = "auto",
        min_confidence: float = 0.6,
        max_tags_per_source: int = 10,
        enable_keywords: bool = True,
        enable_entities: bool = True,
        enable_topics: bool = True,
        enable_rules: bool = True,
    ):
        """Initialize auto-tagger.

        Args:
            keyword_method: Keyword extraction method ('tfidf', 'rake', 'keybert', 'auto')
            min_confidence: Minimum confidence threshold (0.0 - 1.0)
            max_tags_per_source: Maximum tags per extraction method
            enable_keywords: Enable keyword extraction
            enable_entities: Enable entity extraction
            enable_topics: Enable topic modeling
            enable_rules: Enable rule-based tagging
        """
        self.min_confidence = min_confidence
        self.max_tags_per_source = max_tags_per_source

        self.enable_keywords = enable_keywords
        self.enable_entities = enable_entities
        self.enable_topics = enable_topics
        self.enable_rules = enable_rules

        if enable_keywords:
            self.keyword_extractor = KeywordExtractor(
                method=keyword_method, top_k=max_tags_per_source
            )
        else:
            self.keyword_extractor = None

        if enable_entities:
            self.entity_extractor = EntityExtractor(min_confidence=min_confidence)
        else:
            self.entity_extractor = None

        if enable_topics:
            self.topic_modeler = TopicModeler()
        else:
            self.topic_modeler = None

        if enable_rules:
            self.rule_tagger = RuleBasedTagger()
            self.rule_tagger.rules = TagRule.create_default_rules()
        else:
            self.rule_tagger = None

        self.executor = ThreadPoolExecutor(max_workers=4)

    async def tag_document(
        self, text: str, metadata: dict[str, Any] | None = None, file_id: str | None = None
    ) -> list[dict[str, Any]]:
        """Tag a document using all enabled methods.

        Args:
            text: Document text
            metadata: File metadata (extension, size, etc.)
            file_id: Optional file ID for database storage

        Returns:
            List of tag dictionaries with keys:
                - name: Tag name
                - source: Extraction source ('keyword', 'ner', 'topic', 'rule')
                - confidence: Confidence score (0.0 - 1.0)
                - entity_type: Entity type (for NER tags only)
                - context: Text context where tag was found
                - metadata: Additional extraction metadata

        Raises:
            ValueError: If text is empty
        """
        if not text or not text.strip():
            raise ValueError("Text cannot be empty")

        metadata = metadata or {}

        loop = asyncio.get_event_loop()
        tasks = []

        if self.enable_keywords and self.keyword_extractor:
            tasks.append(loop.run_in_executor(self.executor, self._extract_keywords, text))

        if self.enable_entities and self.entity_extractor:
            tasks.append(loop.run_in_executor(self.executor, self._extract_entities, text))

        if self.enable_topics and self.topic_modeler:
            tasks.append(loop.run_in_executor(self.executor, self._extract_topics, text))

        if self.enable_rules and self.rule_tagger:
            tasks.append(loop.run_in_executor(self.executor, self._apply_rules, text, metadata))

        results = await asyncio.gather(*tasks, return_exceptions=True)

        all_tags = []
        for result in results:
            if isinstance(result, list):
                all_tags.extend(result)

        all_tags = self._deduplicate_and_merge(all_tags)

        all_tags = [tag for tag in all_tags if tag["confidence"] >= self.min_confidence]

        all_tags.sort(key=lambda x: x["confidence"], reverse=True)

        return all_tags

    def _extract_keywords(self, text: str) -> list[dict[str, Any]]:
        """Extract keywords.

        Args:
            text: Document text

        Returns:
            List of tag dictionaries
        """
        try:
            keywords = self.keyword_extractor.extract(text)
            return [
                {
                    "name": kw,
                    "source": "keyword",
                    "confidence": conf,
                    "entity_type": None,
                    "context": self._extract_context(text, kw),
                    "metadata": {"algorithm": self.keyword_extractor.method},
                }
                for kw, conf in keywords[: self.max_tags_per_source]
            ]
        except Exception:
            return []

    def _extract_entities(self, text: str) -> list[dict[str, Any]]:
        """Extract named entities.

        Args:
            text: Document text

        Returns:
            List of tag dictionaries
        """
        try:
            entities = self.entity_extractor.extract(text)
            return [
                {
                    "name": ent_text,
                    "source": "ner",
                    "confidence": conf,
                    "entity_type": ent_type,
                    "context": self._extract_context(text, ent_text),
                    "metadata": {"model": self.entity_extractor.model_name},
                }
                for ent_text, ent_type, conf in entities[: self.max_tags_per_source]
            ]
        except Exception:
            return []

    def _extract_topics(self, text: str) -> list[dict[str, Any]]:
        """Extract topics.

        Args:
            text: Document text

        Returns:
            List of tag dictionaries
        """
        try:
            topics = self.topic_modeler.extract_topics(text, top_n=min(3, self.max_tags_per_source))
            return [
                {
                    "name": topic_label,
                    "source": "topic",
                    "confidence": conf,
                    "entity_type": None,
                    "context": None,
                    "metadata": {"n_topics": self.topic_modeler.n_topics},
                }
                for topic_label, conf in topics
            ]
        except Exception:
            return []

    def _apply_rules(self, text: str, metadata: dict[str, Any]) -> list[dict[str, Any]]:
        """Apply rule-based tagging.

        Args:
            text: Document text
            metadata: File metadata

        Returns:
            List of tag dictionaries
        """
        try:
            rule_tags = self.rule_tagger.apply_rules(text, metadata)
            return [
                {
                    "name": tag_name,
                    "source": "rule",
                    "confidence": conf,
                    "entity_type": None,
                    "context": None,
                    "metadata": {"rule_based": True},
                }
                for tag_name, conf in rule_tags[: self.max_tags_per_source]
            ]
        except Exception:
            return []

    def _extract_context(self, text: str, keyword: str, window: int = 50) -> str | None:
        """Extract text context around a keyword.

        Args:
            text: Full document text
            keyword: Keyword to find
            window: Number of characters before/after keyword

        Returns:
            Context string or None if keyword not found
        """
        try:
            pos = text.lower().find(keyword.lower())
            if pos == -1:
                return None

            start = max(0, pos - window)
            end = min(len(text), pos + len(keyword) + window)

            context = text[start:end]

            if start > 0:
                context = "..." + context
            if end < len(text):
                context = context + "..."

            return context
        except Exception:
            return None

    def _deduplicate_and_merge(self, tags: list[dict[str, Any]]) -> list[dict[str, Any]]:
        """Deduplicate and merge similar tags.

        Args:
            tags: List of tag dictionaries

        Returns:
            Deduplicated and merged list of tags
        """
        tag_map = {}

        for tag in tags:
            name_lower = tag["name"].lower()

            if name_lower in tag_map:
                existing = tag_map[name_lower]

                if tag["confidence"] > existing["confidence"] or (
                    tag["source"] == "ner" and existing["source"] != "ner"
                ):
                    tag_map[name_lower] = tag
            else:
                tag_map[name_lower] = tag

        return list(tag_map.values())

    async def batch_tag_documents(
        self, documents: list[tuple[str, str, dict[str, Any] | None]]
    ) -> list[tuple[str, list[dict[str, Any]]]]:
        """Tag multiple documents in parallel.

        Args:
            documents: List of (file_id, text, metadata) tuples

        Returns:
            List of (file_id, tags) tuples
        """
        tasks = [
            self.tag_document(text, metadata, file_id) for file_id, text, metadata in documents
        ]

        results = await asyncio.gather(*tasks, return_exceptions=True)

        output = []
        for i, result in enumerate(results):
            file_id = documents[i][0]
            if isinstance(result, list):
                output.append((file_id, result))
            else:
                output.append((file_id, []))

        return output

    def add_custom_rule(self, rule: TagRule) -> None:
        """Add a custom tagging rule.

        Args:
            rule: TagRule to add
        """
        if self.rule_tagger:
            self.rule_tagger.add_rule(rule)

    def remove_custom_rule(self, name: str) -> bool:
        """Remove a custom tagging rule.

        Args:
            name: Rule name to remove

        Returns:
            True if removed, False if not found
        """
        if self.rule_tagger:
            return self.rule_tagger.remove_rule(name)
        return False

    def get_statistics(self, tags: list[dict[str, Any]]) -> dict[str, Any]:
        """Get statistics about extracted tags.

        Args:
            tags: List of tag dictionaries

        Returns:
            Dictionary with statistics

        Example:
            >>> stats = tagger.get_statistics(tags)
            >>> print(stats)
            {
                "total_tags": 15,
                "by_source": {"keyword": 6, "ner": 5, "topic": 2, "rule": 2},
                "avg_confidence": 0.82,
                "by_entity_type": {"person": 2, "organization": 3}
            }
        """
        if not tags:
            return {"total_tags": 0, "by_source": {}, "avg_confidence": 0.0, "by_entity_type": {}}

        by_source = {}
        by_entity_type = {}
        confidences = []

        for tag in tags:
            source = tag["source"]
            by_source[source] = by_source.get(source, 0) + 1

            confidences.append(tag["confidence"])

            if tag.get("entity_type"):
                etype = tag["entity_type"]
                by_entity_type[etype] = by_entity_type.get(etype, 0) + 1

        return {
            "total_tags": len(tags),
            "by_source": by_source,
            "avg_confidence": sum(confidences) / len(confidences),
            "by_entity_type": by_entity_type,
        }
