"""Named Entity Recognition for document tagging.

This module extracts named entities (people, organizations, locations, etc.)
using spaCy or a fallback regex-based approach.

Contract:
    Input: Text string
    Output: List of (entity_text, entity_type, confidence) tuples
    Performance: < 500ms for 1000 words
"""

from collections import Counter
import re

try:
    import spacy

    SPACY_AVAILABLE = True
except ImportError:
    SPACY_AVAILABLE = False


class EntityExtractor:
    """Extract named entities from text.

    Uses spaCy for high-quality NER if available, falls back to
    simple regex-based extraction.

    Attributes:
        model_name: spaCy model name ('en_core_web_sm', 'en_core_web_md', etc.)
        min_confidence: Minimum confidence threshold
        entity_types: Entity types to extract

    Example:
        >>> extractor = EntityExtractor()
        >>> entities = extractor.extract("Apple Inc. CEO Tim Cook spoke...")
        >>> print(entities)
        [
            ("Apple Inc.", "ORGANIZATION", 0.95),
            ("Tim Cook", "PERSON", 0.92)
        ]
    """

    ENTITY_TYPE_MAP = {
        "PERSON": "person",
        "ORG": "organization",
        "GPE": "location",
        "LOC": "location",
        "DATE": "date",
        "EVENT": "event",
        "PRODUCT": "product",
        "WORK_OF_ART": "other",
        "LAW": "other",
        "LANGUAGE": "other",
        "FAC": "location",
        "NORP": "organization",
    }

    def __init__(
        self,
        model_name: str = "en_core_web_sm",
        min_confidence: float = 0.7,
        entity_types: list[str] | None = None,
    ):
        """Initialize entity extractor.

        Args:
            model_name: spaCy model name
            min_confidence: Minimum confidence threshold (0.0 - 1.0)
            entity_types: Entity types to extract (None = all)
        """
        self.model_name = model_name
        self.min_confidence = min_confidence
        self.entity_types = entity_types or list(self.ENTITY_TYPE_MAP.keys())

        self.nlp = None
        if SPACY_AVAILABLE:
            try:
                self.nlp = spacy.load(model_name)
            except Exception:
                try:
                    self.nlp = spacy.load("en_core_web_sm")
                except Exception:
                    pass

    def extract(self, text: str) -> list[tuple[str, str, float]]:
        """Extract named entities from text.

        Args:
            text: Input text

        Returns:
            List of (entity_text, entity_type, confidence) tuples

        Raises:
            ValueError: If text is empty or invalid
        """
        if not text or not text.strip():
            raise ValueError("Text cannot be empty")

        if self.nlp:
            return self._extract_spacy(text)
        return self._extract_regex(text)

    def _extract_spacy(self, text: str) -> list[tuple[str, str, float]]:
        """Extract entities using spaCy.

        Args:
            text: Input text

        Returns:
            List of (entity_text, entity_type, confidence) tuples
        """
        doc = self.nlp(text)

        entities = []
        for ent in doc.ents:
            if ent.label_ in self.entity_types:
                entity_type = self.ENTITY_TYPE_MAP.get(ent.label_, "other")

                confidence = getattr(ent, "score", 0.9)

                if confidence >= self.min_confidence:
                    entities.append((ent.text.strip(), entity_type, float(confidence)))

        entities = self._deduplicate_entities(entities)
        entities.sort(key=lambda x: x[2], reverse=True)

        return entities

    def _extract_regex(self, text: str) -> list[tuple[str, str, float]]:
        """Extract entities using regex patterns (fallback).

        Args:
            text: Input text

        Returns:
            List of (entity_text, entity_type, confidence) tuples
        """
        entities = []

        org_pattern = r"\b([A-Z][a-z]+ (?:Inc\.|Corp\.|LLC|Ltd\.|Co\.))\b"
        for match in re.finditer(org_pattern, text):
            entities.append((match.group(1), "organization", 0.6))

        person_pattern = r"\b([A-Z][a-z]+ [A-Z][a-z]+)\b"
        for match in re.finditer(person_pattern, text):
            name = match.group(1)
            if not any(word in name for word in ["Inc", "Corp", "LLC"]):
                entities.append((name, "person", 0.5))

        location_pattern = r"\b([A-Z][a-z]+(?: [A-Z][a-z]+)?(?:, [A-Z]{2})?)\b"
        for match in re.finditer(location_pattern, text):
            entities.append((match.group(1), "location", 0.5))

        entities = self._deduplicate_entities(entities)
        entities = [e for e in entities if e[2] >= self.min_confidence]
        entities.sort(key=lambda x: x[2], reverse=True)

        return entities[:50]

    def _deduplicate_entities(
        self, entities: list[tuple[str, str, float]]
    ) -> list[tuple[str, str, float]]:
        """Remove duplicate entities, keeping highest confidence.

        Args:
            entities: List of (entity_text, entity_type, confidence) tuples

        Returns:
            Deduplicated list of entities
        """
        entity_map = {}
        for text, etype, conf in entities:
            key = (text.lower(), etype)
            if key not in entity_map or conf > entity_map[key][2]:
                entity_map[key] = (text, etype, conf)

        return list(entity_map.values())

    def extract_by_type(self, text: str, entity_type: str) -> list[tuple[str, float]]:
        """Extract entities of a specific type.

        Args:
            text: Input text
            entity_type: Type to filter by ('person', 'organization', etc.)

        Returns:
            List of (entity_text, confidence) tuples
        """
        all_entities = self.extract(text)
        filtered = [(text, conf) for text, etype, conf in all_entities if etype == entity_type]
        return filtered

    def get_entity_counts(self, text: str) -> dict:
        """Get entity counts by type.

        Args:
            text: Input text

        Returns:
            Dictionary mapping entity types to counts

        Example:
            >>> counts = extractor.get_entity_counts(text)
            >>> print(counts)
            {'person': 5, 'organization': 3, 'location': 2}
        """
        entities = self.extract(text)
        counts = Counter(etype for _, etype, _ in entities)
        return dict(counts)
