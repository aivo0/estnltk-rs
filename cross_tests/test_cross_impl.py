"""
Cross-implementation tests: run both EstNLTK (Python) and Rust on identical
inputs and assert identical outputs.

These tests verify that the Rust RegexTagger produces the same spans and
annotations as the original Python EstNLTK RegexTagger for group=0 patterns.
"""

import regex
import pytest

from estnltk import Text
from estnltk.converters import layer_to_dict
from estnltk.taggers.system.rule_taggers import StaticExtractionRule, Ruleset, RegexTagger

import estnltk_regex_rs
from estnltk_regex_rs import RsRegexTagger, rs_regex_tag


def run_estnltk_regex_tagger(text_str, rules_spec, conflict_resolver="KEEP_MAXIMAL",
                              lowercase_text=False, output_attributes=None):
    """Run EstNLTK's RegexTagger and return normalized span list.

    rules_spec: list of dicts with keys: pattern, attributes, group, priority
    Returns: list of {"base_span": (start, end), "annotations": [{...}]}
    """
    ruleset = Ruleset()
    rules = []
    for r in rules_spec:
        pattern = regex.Regex(r["pattern"])
        attrs = r.get("attributes", {})
        group = r.get("group", 0)
        priority = r.get("priority", 0)
        rules.append(StaticExtractionRule(
            pattern=pattern, attributes=attrs, group=group, priority=priority
        ))
    ruleset.add_rules(rules)

    if output_attributes is None:
        # Collect all attribute keys
        all_keys = set()
        for r in rules_spec:
            all_keys.update(r.get("attributes", {}).keys())
        output_attributes = sorted(all_keys) if all_keys else ()

    tagger = RegexTagger(
        ruleset=ruleset,
        output_layer="test_layer",
        output_attributes=output_attributes,
        conflict_resolver=conflict_resolver,
        lowercase_text=lowercase_text,
    )

    text = Text(text_str)
    tagger.tag(text)
    layer_dict = layer_to_dict(text["test_layer"])

    # Normalize: extract just base_span and annotations (without 'match' attribute)
    result = []
    for span_info in layer_dict["spans"]:
        annotations = []
        for ann in span_info["annotations"]:
            # Remove the 'match' attribute (Python re.Match object, not portable)
            clean_ann = {k: v for k, v in ann.items() if k != "match"}
            annotations.append(clean_ann)
        result.append({
            "base_span": tuple(span_info["base_span"]),
            "annotations": annotations,
        })
    return result


def run_rust_regex_tagger(text_str, rules_spec, conflict_resolver="KEEP_MAXIMAL",
                           lowercase_text=False, output_attributes=None):
    """Run Rust RegexTagger and return normalized span list."""
    patterns = []
    for r in rules_spec:
        patterns.append({
            "pattern": r["pattern"],
            "attributes": r.get("attributes", {}),
            "group": r.get("group", 0),
            "priority": r.get("priority", 0),
        })

    if output_attributes is None:
        all_keys = set()
        for r in rules_spec:
            all_keys.update(r.get("attributes", {}).keys())
        output_attributes = sorted(all_keys) if all_keys else []

    tagger = RsRegexTagger(
        patterns=patterns,
        output_layer="test_layer",
        output_attributes=output_attributes,
        conflict_resolver=conflict_resolver,
        lowercase_text=lowercase_text,
    )
    layer_dict = tagger.tag(text_str)

    result = []
    for span_info in layer_dict["spans"]:
        annotations = []
        for ann in span_info["annotations"]:
            annotations.append(dict(ann))
        result.append({
            "base_span": tuple(span_info["base_span"]),
            "annotations": annotations,
        })
    return result


# ============================================================
# Test cases
# ============================================================

class TestBasicEmailMatching:
    """Test 1: Basic email matching with group=0."""

    TEXT = "Aadressilt bla@bla.ee tuli"
    PATTERN = r"[a-zA-Z0-9_.+-]+@[a-zA-Z0-9-]+\.[a-zA-Z0-9-.]+"
    RULES = [{"pattern": PATTERN, "attributes": {"comment": "e-mail"}, "group": 0, "priority": 0}]

    def test_estnltk_finds_email(self):
        result = run_estnltk_regex_tagger(self.TEXT, self.RULES)
        assert len(result) == 1
        assert result[0]["base_span"] == (11, 21)

    def test_rust_finds_email(self):
        result = run_rust_regex_tagger(self.TEXT, self.RULES)
        assert len(result) == 1
        assert result[0]["base_span"] == (11, 21)

    def test_cross_impl_email(self):
        estnltk_result = run_estnltk_regex_tagger(self.TEXT, self.RULES)
        rust_result = run_rust_regex_tagger(self.TEXT, self.RULES)
        assert estnltk_result == rust_result


class TestNumberMatching:
    """Test 2: Simple number pattern (resharp-compatible, no capture groups)."""

    TEXT = "Hind on 10 456 ja 789 krooni"
    # resharp-compatible: no capture groups, no lazy quantifiers
    PATTERN = r"-?[0-9]+"
    RULES = [{"pattern": PATTERN, "attributes": {"comment": "number"}, "group": 0, "priority": 0}]

    def test_cross_impl_numbers(self):
        estnltk_result = run_estnltk_regex_tagger(self.TEXT, self.RULES)
        rust_result = run_rust_regex_tagger(self.TEXT, self.RULES)
        assert estnltk_result == rust_result


class TestConflictResolutionKeepAll:
    """Test 3: KEEP_ALL with overlapping regex patterns."""

    TEXT = "Muna ja kana."
    RULES = [
        {"pattern": "m..a.ja", "attributes": {"_priority_": 0}, "group": 0, "priority": 0},
        {"pattern": "ja", "attributes": {"_priority_": 0}, "group": 0, "priority": 0},
        {"pattern": "ja.k..a", "attributes": {"_priority_": 0}, "group": 0, "priority": 0},
    ]

    def test_estnltk_keep_all(self):
        result = run_estnltk_regex_tagger(self.TEXT, self.RULES,
                                           conflict_resolver="KEEP_ALL",
                                           lowercase_text=True)
        assert len(result) == 3
        assert result[0]["base_span"] == (0, 7)
        assert result[1]["base_span"] == (5, 7)
        assert result[2]["base_span"] == (5, 12)

    def test_rust_keep_all(self):
        result = run_rust_regex_tagger(self.TEXT, self.RULES,
                                        conflict_resolver="KEEP_ALL",
                                        lowercase_text=True)
        assert len(result) == 3
        assert result[0]["base_span"] == (0, 7)
        assert result[1]["base_span"] == (5, 7)
        assert result[2]["base_span"] == (5, 12)

    def test_cross_impl_keep_all(self):
        estnltk_result = run_estnltk_regex_tagger(self.TEXT, self.RULES,
                                                    conflict_resolver="KEEP_ALL",
                                                    lowercase_text=True)
        rust_result = run_rust_regex_tagger(self.TEXT, self.RULES,
                                             conflict_resolver="KEEP_ALL",
                                             lowercase_text=True)
        assert estnltk_result == rust_result


class TestConflictResolutionKeepMaximal:
    """Test 4: KEEP_MAXIMAL with overlapping patterns."""

    TEXT = "Muna ja kana."
    RULES = [
        {"pattern": "m..a.ja", "attributes": {"_priority_": 0}, "group": 0, "priority": 0},
        {"pattern": "ja", "attributes": {"_priority_": 0}, "group": 0, "priority": 0},
        {"pattern": "ja.k..a", "attributes": {"_priority_": 0}, "group": 0, "priority": 0},
    ]

    def test_estnltk_keep_maximal(self):
        result = run_estnltk_regex_tagger(self.TEXT, self.RULES,
                                           conflict_resolver="KEEP_MAXIMAL",
                                           lowercase_text=True)
        assert len(result) == 2
        assert result[0]["base_span"] == (0, 7)
        assert result[1]["base_span"] == (5, 12)

    def test_rust_keep_maximal(self):
        result = run_rust_regex_tagger(self.TEXT, self.RULES,
                                        conflict_resolver="KEEP_MAXIMAL",
                                        lowercase_text=True)
        assert len(result) == 2
        assert result[0]["base_span"] == (0, 7)
        assert result[1]["base_span"] == (5, 12)

    def test_cross_impl_keep_maximal(self):
        estnltk_result = run_estnltk_regex_tagger(self.TEXT, self.RULES,
                                                    conflict_resolver="KEEP_MAXIMAL",
                                                    lowercase_text=True)
        rust_result = run_rust_regex_tagger(self.TEXT, self.RULES,
                                             conflict_resolver="KEEP_MAXIMAL",
                                             lowercase_text=True)
        assert estnltk_result == rust_result


class TestConflictResolutionKeepMinimal:
    """Test 5: KEEP_MINIMAL with overlapping patterns."""

    TEXT = "Muna ja kana."
    RULES = [
        {"pattern": "m..a.ja", "attributes": {"_priority_": 0}, "group": 0, "priority": 0},
        {"pattern": "ja", "attributes": {"_priority_": 0}, "group": 0, "priority": 0},
        {"pattern": "ja.k..a", "attributes": {"_priority_": 0}, "group": 0, "priority": 0},
    ]

    def test_estnltk_keep_minimal(self):
        result = run_estnltk_regex_tagger(self.TEXT, self.RULES,
                                           conflict_resolver="KEEP_MINIMAL",
                                           lowercase_text=True)
        assert len(result) == 1
        assert result[0]["base_span"] == (5, 7)

    def test_rust_keep_minimal(self):
        result = run_rust_regex_tagger(self.TEXT, self.RULES,
                                        conflict_resolver="KEEP_MINIMAL",
                                        lowercase_text=True)
        assert len(result) == 1
        assert result[0]["base_span"] == (5, 7)

    def test_cross_impl_keep_minimal(self):
        estnltk_result = run_estnltk_regex_tagger(self.TEXT, self.RULES,
                                                    conflict_resolver="KEEP_MINIMAL",
                                                    lowercase_text=True)
        rust_result = run_rust_regex_tagger(self.TEXT, self.RULES,
                                             conflict_resolver="KEEP_MINIMAL",
                                             lowercase_text=True)
        assert estnltk_result == rust_result


class TestLowercaseText:
    """Test 6: lowercase_text flag matches case-insensitively."""

    TEXT = "HELLO World hello"
    RULES = [{"pattern": "hello", "attributes": {}, "group": 0, "priority": 0}]

    def test_cross_impl_lowercase(self):
        estnltk_result = run_estnltk_regex_tagger(self.TEXT, self.RULES,
                                                    lowercase_text=True)
        rust_result = run_rust_regex_tagger(self.TEXT, self.RULES,
                                             lowercase_text=True)
        assert estnltk_result == rust_result

    def test_rust_lowercase_spans(self):
        """Verify spans reference original text positions."""
        result = run_rust_regex_tagger(self.TEXT, self.RULES, lowercase_text=True)
        assert len(result) == 2
        assert result[0]["base_span"] == (0, 5)
        assert result[1]["base_span"] == (12, 17)


class TestEstonianMultibyte:
    """Test 7: Estonian text with multi-byte UTF-8 characters."""

    TEXT = "Tüüpiline näide öökülma kohta"
    RULES = [{"pattern": "öökülma", "attributes": {"type": "word"}, "group": 0, "priority": 0}]

    def test_cross_impl_multibyte(self):
        estnltk_result = run_estnltk_regex_tagger(self.TEXT, self.RULES)
        rust_result = run_rust_regex_tagger(self.TEXT, self.RULES)
        assert estnltk_result == rust_result

    def test_rust_char_offsets(self):
        """Verify character offsets (not byte offsets)."""
        result = run_rust_regex_tagger(self.TEXT, self.RULES)
        assert len(result) == 1
        # Char positions: ö starts at char 16 in this text
        assert result[0]["base_span"] == (16, 23)


class TestEstonianMultiplePatterns:
    """Test Estonian text with multiple patterns containing multi-byte chars."""

    TEXT = "Jõgeva mõõtmisel õues"
    RULES = [
        {"pattern": "õ[a-zõ]+", "attributes": {"type": "õ-word"}, "group": 0, "priority": 0},
    ]

    def test_cross_impl(self):
        estnltk_result = run_estnltk_regex_tagger(self.TEXT, self.RULES)
        rust_result = run_rust_regex_tagger(self.TEXT, self.RULES)
        assert estnltk_result == rust_result


class TestPriorityResolution:
    """Test 8: KEEP_ALL_EXCEPT_PRIORITY and KEEP_MAXIMAL_EXCEPT_PRIORITY."""

    TEXT = "hello world"
    RULES = [
        {"pattern": "[a-z]+", "attributes": {"label": "low"}, "group": 0, "priority": 1},
        {"pattern": "[a-z]+", "attributes": {"label": "high"}, "group": 0, "priority": 0},
    ]

    def test_rust_keep_all_except_priority(self):
        """Higher priority number (lower precedence) should be removed."""
        result = run_rust_regex_tagger(self.TEXT, self.RULES,
                                        conflict_resolver="KEEP_ALL_EXCEPT_PRIORITY")
        # Both patterns produce identical spans. Priority 1 > priority 0,
        # so priority=1 entries should be removed for overlapping spans in same group.
        for span in result:
            for ann in span["annotations"]:
                assert ann["label"] == "high"

    def test_rust_keep_maximal_except_priority(self):
        result = run_rust_regex_tagger(self.TEXT, self.RULES,
                                        conflict_resolver="KEEP_MAXIMAL_EXCEPT_PRIORITY")
        for span in result:
            for ann in span["annotations"]:
                assert ann["label"] == "high"


class TestEmptyText:
    """Edge case: empty text."""

    def test_rust_empty_text(self):
        result = run_rust_regex_tagger("", [{"pattern": "abc"}])
        assert result == []

    def test_cross_impl_empty(self):
        rules = [{"pattern": "abc", "attributes": {}, "group": 0, "priority": 0}]
        estnltk_result = run_estnltk_regex_tagger("", rules)
        rust_result = run_rust_regex_tagger("", rules)
        assert estnltk_result == rust_result


class TestNoMatches:
    """Edge case: pattern doesn't match."""

    def test_cross_impl_no_match(self):
        rules = [{"pattern": "xyz", "attributes": {}, "group": 0, "priority": 0}]
        estnltk_result = run_estnltk_regex_tagger("hello world", rules)
        rust_result = run_rust_regex_tagger("hello world", rules)
        assert estnltk_result == rust_result
        assert rust_result == []
