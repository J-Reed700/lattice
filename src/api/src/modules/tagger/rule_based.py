"""Rule-based tagging using custom patterns.

This module allows users to define custom tagging rules based on
regex patterns, file metadata, or content patterns.

Contract:
    Input: Text + metadata + rules
    Output: List of matched tags with confidence
    Performance: < 100ms per document
"""

import re
from typing import Any


class TagRule:
    """A single tagging rule.

    Attributes:
        name: Tag name to apply
        pattern: Regex pattern to match
        metadata_conditions: Conditions on file metadata
        confidence: Confidence score for this rule
        case_sensitive: Whether pattern matching is case-sensitive

    Example:
        >>> rule = TagRule(
        ...     name="meeting-notes",
        ...     pattern=r"meeting|agenda|attendees",
        ...     confidence=0.9
        ... )
    """

    def __init__(
        self,
        name: str,
        pattern: str | None = None,
        metadata_conditions: dict[str, Any] | None = None,
        confidence: float = 0.8,
        case_sensitive: bool = False,
    ):
        """Initialize tag rule.

        Args:
            name: Tag name to apply when rule matches
            pattern: Regex pattern to match against text
            metadata_conditions: Conditions on file metadata
            confidence: Confidence score (0.0 - 1.0)
            case_sensitive: Whether matching is case-sensitive
        """
        self.name = name
        self.pattern = pattern
        self.metadata_conditions = metadata_conditions or {}
        self.confidence = confidence
        self.case_sensitive = case_sensitive

        self._compiled_pattern = None
        if pattern:
            flags = 0 if case_sensitive else re.IGNORECASE
            self._compiled_pattern = re.compile(pattern, flags)

    def matches(self, text: str, metadata: dict[str, Any] | None = None) -> bool:
        """Check if rule matches given text and metadata.

        Args:
            text: Document text
            metadata: File metadata dictionary

        Returns:
            True if rule matches, False otherwise
        """
        metadata = metadata or {}

        if self._compiled_pattern:
            if not self._compiled_pattern.search(text):
                return False

        for key, expected_value in self.metadata_conditions.items():
            actual_value = metadata.get(key)

            if isinstance(expected_value, (list, tuple)):
                if actual_value not in expected_value:
                    return False
            elif isinstance(expected_value, dict):
                if "min" in expected_value and actual_value < expected_value["min"]:
                    return False
                if "max" in expected_value and actual_value > expected_value["max"]:
                    return False
            elif actual_value != expected_value:
                return False

        return True

    def to_dict(self) -> dict[str, Any]:
        """Convert rule to dictionary."""
        return {
            "name": self.name,
            "pattern": self.pattern,
            "metadata_conditions": self.metadata_conditions,
            "confidence": self.confidence,
            "case_sensitive": self.case_sensitive,
        }

    @classmethod
    def from_dict(cls, data: dict[str, Any]) -> "TagRule":
        """Create rule from dictionary."""
        return cls(
            name=data["name"],
            pattern=data.get("pattern"),
            metadata_conditions=data.get("metadata_conditions"),
            confidence=data.get("confidence", 0.8),
            case_sensitive=data.get("case_sensitive", False),
        )


class RuleBasedTagger:
    """Apply custom tagging rules to documents.

    Allows users to define and apply custom tagging rules based on
    patterns, file types, sizes, dates, etc.

    Attributes:
        rules: List of TagRule objects

    Example:
        >>> tagger = RuleBasedTagger()
        >>> tagger.add_rule(TagRule(
        ...     name="invoice",
        ...     pattern=r"invoice|bill|payment",
        ...     metadata_conditions={"extension": ["pdf", "doc"]}
        ... ))
        >>> tags = tagger.apply_rules(text, {"extension": "pdf"})
    """

    def __init__(self, rules: list[TagRule] | None = None):
        """Initialize rule-based tagger.

        Args:
            rules: Initial list of tagging rules
        """
        self.rules = rules or []

    def add_rule(self, rule: TagRule) -> None:
        """Add a tagging rule.

        Args:
            rule: TagRule to add
        """
        self.rules.append(rule)

    def remove_rule(self, name: str) -> bool:
        """Remove a tagging rule by name.

        Args:
            name: Name of rule to remove

        Returns:
            True if rule was removed, False if not found
        """
        original_len = len(self.rules)
        self.rules = [r for r in self.rules if r.name != name]
        return len(self.rules) < original_len

    def apply_rules(
        self, text: str, metadata: dict[str, Any] | None = None
    ) -> list[tuple[str, float]]:
        """Apply all rules to text and metadata.

        Args:
            text: Document text
            metadata: File metadata dictionary

        Returns:
            List of (tag_name, confidence) tuples for matched rules
        """
        matched_tags = []

        for rule in self.rules:
            if rule.matches(text, metadata):
                matched_tags.append((rule.name, rule.confidence))

        matched_tags = self._deduplicate_tags(matched_tags)
        matched_tags.sort(key=lambda x: x[1], reverse=True)

        return matched_tags

    def _deduplicate_tags(self, tags: list[tuple[str, float]]) -> list[tuple[str, float]]:
        """Remove duplicate tags, keeping highest confidence.

        Args:
            tags: List of (tag_name, confidence) tuples

        Returns:
            Deduplicated list of tags
        """
        tag_map = {}
        for name, conf in tags:
            if name not in tag_map or conf > tag_map[name]:
                tag_map[name] = conf

        return [(name, conf) for name, conf in tag_map.items()]

    def export_rules(self) -> list[dict[str, Any]]:
        """Export rules as list of dictionaries.

        Returns:
            List of rule dictionaries
        """
        return [rule.to_dict() for rule in self.rules]

    def import_rules(self, rules_data: list[dict[str, Any]]) -> None:
        """Import rules from list of dictionaries.

        Args:
            rules_data: List of rule dictionaries
        """
        self.rules = [TagRule.from_dict(data) for data in rules_data]

    def get_rule(self, name: str) -> TagRule | None:
        """Get rule by name.

        Args:
            name: Rule name

        Returns:
            TagRule if found, None otherwise
        """
        for rule in self.rules:
            if rule.name == name:
                return rule
        return None

    @staticmethod
    def create_default_rules() -> list[TagRule]:
        """Create a set of useful default rules.

        Returns:
            List of default TagRule objects
        """
        return [
            TagRule(
                name="meeting-notes",
                pattern=r"\b(meeting|agenda|attendees|action items?)\b",
                confidence=0.85,
            ),
            TagRule(
                name="invoice",
                pattern=r"\b(invoice|bill|payment|due date|amount due)\b",
                confidence=0.90,
            ),
            TagRule(
                name="contract",
                pattern=r"\b(contract|agreement|terms|conditions|parties)\b",
                confidence=0.85,
            ),
            TagRule(
                name="research-paper",
                pattern=r"\b(abstract|introduction|methodology|results|conclusion|references)\b",
                confidence=0.88,
            ),
            TagRule(
                name="code-documentation",
                pattern=r"\b(API|function|class|method|parameter|return)\b",
                metadata_conditions={"extension": ["md", "rst", "txt"]},
                confidence=0.82,
            ),
            TagRule(
                name="financial",
                pattern=r"\b(revenue|profit|loss|budget|forecast|quarterly)\b",
                confidence=0.85,
            ),
            TagRule(
                name="legal",
                pattern=r"\b(plaintiff|defendant|court|lawsuit|judgment|litigation)\b",
                confidence=0.87,
            ),
            TagRule(
                name="presentation",
                pattern=r"\b(slide|presentation|deck|keynote)\b",
                metadata_conditions={"extension": ["ppt", "pptx", "key"]},
                confidence=0.90,
            ),
            TagRule(
                name="email", pattern=r"\b(from:|to:|subject:|sent:|received:)\b", confidence=0.85
            ),
            TagRule(
                name="report",
                pattern=r"\b(executive summary|findings|recommendations|analysis)\b",
                confidence=0.82,
            ),
        ]
