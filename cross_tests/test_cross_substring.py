"""
Cross-implementation tests: run both EstNLTK SubstringTagger (Python) and
Rust RsSubstringTagger on identical inputs and assert identical outputs.

These tests verify that the Rust SubstringTagger produces the same spans and
annotations as the original Python EstNLTK SubstringTagger for static rules.
"""

import pytest

from estnltk import Text
from estnltk.taggers.system.rule_taggers import (
    StaticExtractionRule, Ruleset, SubstringTagger,
)
from estnltk.taggers.system.rule_taggers.extraction_rules.ambiguous_ruleset import (
    AmbiguousRuleset,
)

from estnltk_regex_rs import RsSubstringTagger


def run_estnltk_substring_tagger(
    text_str, rules_spec, conflict_resolver="KEEP_MAXIMAL",
    ignore_case=False, token_separators="", output_attributes=None,
):
    """Run EstNLTK's SubstringTagger and return normalized span list."""
    rules = []
    for r in rules_spec:
        attrs = r.get("attributes", {})
        group = r.get("group", 0)
        priority = r.get("priority", 0)
        rules.append(StaticExtractionRule(
            pattern=r["pattern"], attributes=attrs, group=group, priority=priority,
        ))

    if output_attributes is None:
        all_keys = set()
        for r in rules_spec:
            all_keys.update(r.get("attributes", {}).keys())
        output_attributes = tuple(sorted(all_keys)) if all_keys else ()

    ruleset = Ruleset(rules)
    tagger = SubstringTagger(
        ruleset=ruleset,
        output_layer="test_layer",
        output_attributes=output_attributes,
        conflict_resolver=conflict_resolver,
        ignore_case=ignore_case,
        token_separators=token_separators,
    )
    text = Text(text_str)
    tagger(text)

    layer = text["test_layer"]
    result = []
    for span in layer:
        annotations = []
        for ann in span.annotations:
            annotations.append(dict(ann))
        result.append({
            "base_span": (span.start, span.end),
            "annotations": annotations,
        })
    return result


def run_rust_substring_tagger(
    text_str, rules_spec, conflict_resolver="KEEP_MAXIMAL",
    lowercase_text=False, token_separators="", output_attributes=None,
):
    """Run Rust RsSubstringTagger and return normalized span list."""
    patterns = []
    for r in rules_spec:
        d = {"pattern": r["pattern"]}
        if "attributes" in r:
            d["attributes"] = r["attributes"]
        if "group" in r:
            d["group"] = r["group"]
        if "priority" in r:
            d["priority"] = r["priority"]
        patterns.append(d)

    if output_attributes is None:
        all_keys = set()
        for r in rules_spec:
            all_keys.update(r.get("attributes", {}).keys())
        output_attributes = sorted(all_keys) if all_keys else []

    tagger = RsSubstringTagger(
        patterns=patterns,
        output_layer="test_layer",
        output_attributes=output_attributes,
        conflict_resolver=conflict_resolver,
        lowercase_text=lowercase_text,
        token_separators=token_separators,
    )
    result_dict = tagger.tag(text_str)

    result = []
    for span in result_dict["spans"]:
        result.append({
            "base_span": span["base_span"],
            "annotations": [dict(a) for a in span["annotations"]],
        })
    return result


def assert_results_equal(py_result, rs_result, msg=""):
    """Compare Python and Rust results span-by-span."""
    assert len(py_result) == len(rs_result), \
        f"Span count mismatch: Python={len(py_result)}, Rust={len(rs_result)}. {msg}"
    for i, (py_span, rs_span) in enumerate(zip(py_result, rs_result)):
        assert py_span["base_span"] == tuple(rs_span["base_span"]), \
            f"Span {i} base_span mismatch: Python={py_span['base_span']}, Rust={rs_span['base_span']}. {msg}"
        assert len(py_span["annotations"]) == len(rs_span["annotations"]), \
            f"Span {i} annotation count mismatch. {msg}"
        for j, (py_ann, rs_ann) in enumerate(zip(py_span["annotations"], rs_span["annotations"])):
            assert py_ann == rs_ann, \
                f"Span {i} annotation {j} mismatch: Python={py_ann}, Rust={rs_ann}. {msg}"


# ---- Cross-implementation tests ----

class TestSubstringBasicMatching:
    RULES = [
        {"pattern": "first"},
        {"pattern": "firs"},
        {"pattern": "irst"},
        {"pattern": "last"},
    ]
    TEXT = "first second last"

    def test_cross_keep_maximal(self):
        py = run_estnltk_substring_tagger(self.TEXT, self.RULES)
        rs = run_rust_substring_tagger(self.TEXT, self.RULES)
        assert_results_equal(py, rs, "basic KEEP_MAXIMAL")

    def test_span_count(self):
        rs = run_rust_substring_tagger(self.TEXT, self.RULES)
        assert len(rs) == 2


class TestSubstringIgnoreCase:
    RULES = [
        {"pattern": "First"},
        {"pattern": "firs"},
        {"pattern": "irst"},
        {"pattern": "LAST"},
    ]
    TEXT = "first second last"

    def test_cross_ignore_case(self):
        py = run_estnltk_substring_tagger(self.TEXT, self.RULES, ignore_case=True)
        rs = run_rust_substring_tagger(self.TEXT, self.RULES, lowercase_text=True)
        assert_results_equal(py, rs, "ignore_case")


class TestSubstringSeparators:
    RULES = [{"pattern": "match"}]

    def test_cross_pipe_separator(self):
        text = "match|match| match| match| match |match"
        py = run_estnltk_substring_tagger(text, self.RULES, token_separators="|")
        rs = run_rust_substring_tagger(text, self.RULES, token_separators="|")
        assert_results_equal(py, rs, "pipe separator")

    def test_cross_multiple_separators(self):
        text = "match match, :match, match"
        py = run_estnltk_substring_tagger(text, self.RULES, token_separators=" ,:")
        rs = run_rust_substring_tagger(text, self.RULES, token_separators=" ,:")
        assert_results_equal(py, rs, "multiple separators")


class TestSubstringAnnotations:
    RULES = [
        {"pattern": "first", "attributes": {"a": 1, "b": 1}},
        {"pattern": "second", "attributes": {"b": 2, "a": 3}},
        {"pattern": "last", "attributes": {"a": 3, "b": 5}},
    ]
    TEXT = "first second last"

    def test_cross_annotations(self):
        py = run_estnltk_substring_tagger(self.TEXT, self.RULES)
        rs = run_rust_substring_tagger(self.TEXT, self.RULES)
        assert_results_equal(py, rs, "annotations")


class TestSubstringConflictStrategies:
    RULES = [
        {"pattern": "abcd"},
        {"pattern": "abc"},
        {"pattern": "bc"},
        {"pattern": "bcd"},
        {"pattern": "bcde"},
        {"pattern": "f"},
        {"pattern": "ef"},
    ]
    TEXT = "abcdea--efg"

    def test_cross_keep_all(self):
        py = run_estnltk_substring_tagger(self.TEXT, self.RULES, conflict_resolver="KEEP_ALL")
        rs = run_rust_substring_tagger(self.TEXT, self.RULES, conflict_resolver="KEEP_ALL")
        assert_results_equal(py, rs, "KEEP_ALL")

    def test_cross_keep_maximal(self):
        py = run_estnltk_substring_tagger(self.TEXT, self.RULES, conflict_resolver="KEEP_MAXIMAL")
        rs = run_rust_substring_tagger(self.TEXT, self.RULES, conflict_resolver="KEEP_MAXIMAL")
        assert_results_equal(py, rs, "KEEP_MAXIMAL")

    def test_cross_keep_minimal(self):
        py = run_estnltk_substring_tagger(self.TEXT, self.RULES, conflict_resolver="KEEP_MINIMAL")
        rs = run_rust_substring_tagger(self.TEXT, self.RULES, conflict_resolver="KEEP_MINIMAL")
        assert_results_equal(py, rs, "KEEP_MINIMAL")


class TestSubstringEstonianMultibyte:
    def test_cross_estonian_chars(self):
        rules = [{"pattern": "öö"}]
        text = "Tüüpiline öökülma näide"
        py = run_estnltk_substring_tagger(text, rules)
        rs = run_rust_substring_tagger(text, rules)
        assert_results_equal(py, rs, "Estonian multibyte")

    def test_cross_estonian_with_attributes(self):
        rules = [
            {"pattern": "Tartu", "attributes": {"type": "town", "country": "Estonia"}},
            {"pattern": "Tallinn", "attributes": {"type": "capital", "country": "Estonia"}},
        ]
        text = "Tartu ja Tallinn on Eesti linnad"
        py = run_estnltk_substring_tagger(text, rules)
        rs = run_rust_substring_tagger(text, rules)
        assert_results_equal(py, rs, "Estonian with attributes")


class TestSubstringEdgeCases:
    def test_cross_empty_text(self):
        rules = [{"pattern": "hello"}]
        py = run_estnltk_substring_tagger("", rules)
        rs = run_rust_substring_tagger("", rules)
        assert_results_equal(py, rs, "empty text")

    def test_cross_no_match(self):
        rules = [{"pattern": "xyz"}]
        py = run_estnltk_substring_tagger("hello world", rules)
        rs = run_rust_substring_tagger("hello world", rules)
        assert_results_equal(py, rs, "no match")

    def test_cross_single_char_pattern(self):
        rules = [{"pattern": "a"}]
        text = "banana"
        py = run_estnltk_substring_tagger(text, rules, conflict_resolver="KEEP_ALL")
        rs = run_rust_substring_tagger(text, rules, conflict_resolver="KEEP_ALL")
        assert_results_equal(py, rs, "single char")
