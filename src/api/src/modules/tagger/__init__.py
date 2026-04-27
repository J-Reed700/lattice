"""Intelligent auto-tagging module for document organization.

This module provides automatic tag extraction using multiple NLP techniques:
- Keyword extraction (TF-IDF, RAKE, KeyBERT)
- Named Entity Recognition (spaCy)
- Topic modeling (LDA, BERTopic)
- Rule-based pattern matching

Public Interface:
    - AutoTagger: Main class for document tagging
    - KeywordExtractor: Extract keywords from text
    - EntityExtractor: Extract named entities
    - TopicModeler: Generate topic labels
    - RuleBasedTagger: Apply custom tagging rules

Example:
    >>> from modules.tagger import AutoTagger
    >>> tagger = AutoTagger()
    >>> tags = await tagger.tag_document(doc_id, text)
    >>> print(tags)
    [
        {"name": "machine learning", "source": "keyword", "confidence": 0.92},
        {"name": "Google", "source": "ner", "entity_type": "organization", "confidence": 0.95}
    ]
"""

from .entity_extractor import EntityExtractor
from .keyword_extractor import KeywordExtractor
from .rule_based import RuleBasedTagger
from .tagger import AutoTagger
from .topic_modeler import TopicModeler

__all__ = [
    "AutoTagger",
    "EntityExtractor",
    "KeywordExtractor",
    "RuleBasedTagger",
    "TopicModeler",
]
