# EstNLTK vs estnltk-rs: Feature Comparison

Thorough comparison of EstNLTK's `rule_taggers` subsystem with the Rust rewrite in `estnltk-rs`.

Coverage legend: **Full** | **Partial** | **None** | **N/A** (not applicable to Rust)

---

## 1. Tagger Types

| Tagger | EstNLTK | estnltk-rs | Coverage |
|--------|---------|------------|----------|
| RegexTagger | `regex` library, supports capture groups, overlapping | `resharp` DFA engine + `regex` crate two-pass capture group extraction | **Partial** |
| SubstringTagger | Aho-Corasick automaton for multi-string matching | `aho-corasick` automaton, token separators, static rules | **Partial** |
| SpanTagger | Matches input layer attribute values against ruleset | — | **None** |
| PhraseTagger | Matches sequential attribute values (phrase tuples), enveloping layer | — | **None** |

**Notes:**
- RegexTagger and SubstringTagger are ported. The other two operate on existing layers rather than raw text, so they have different input requirements.
- SubstringTagger is ported with static rules, token separators, and all conflict strategies. Decorators and expander are not ported (Python-specific).
- SpanTagger and PhraseTagger depend on EstNLTK's layer/text infrastructure (`input_layer`, `input_attribute`), which has no Rust equivalent.

---

## 2. SubstringTagger — Parameter-by-Parameter Comparison

| Parameter | EstNLTK | estnltk-rs | Coverage | Notes |
|-----------|---------|------------|----------|-------|
| `ruleset` / `patterns` | `AmbiguousRuleset` with `StaticExtractionRule` list | List of pattern dicts | **Full** | Different input format, same information carried |
| `output_layer` | str, default `'terms'` | str, default `"substrings"` | **Full** | Different defaults |
| `output_attributes` | Sequence, default `None` | `Vec<String>`, default auto-collected | **Full** | |
| `conflict_resolver` | str or callable | str only | **Partial** | Custom callable resolvers not supported |
| `ignore_case` / `lowercase_text` | `ignore_case`, bool, default `False` | `lowercase_text`, bool, default `false` | **Full** | Different name, same behavior |
| `token_separators` | str, default `''` | str, default `""` | **Full** | |
| `ambiguous_output_layer` | bool, default `True` | bool, default `true` | **Full** | When `false`, only first annotation per span is kept |
| `global_decorator` | `Callable[[Text, ElementaryBaseSpan, Dict], Optional[Dict]]` | — | **None** | Python callables can't run in Rust |
| `group_attribute` | str, default `None` | `Option<String>`, default `None` | **Full** | |
| `priority_attribute` | str, default `None` | `Option<String>`, default `None` | **Full** | |
| `pattern_attribute` | str, default `None` | `Option<String>`, default `None` | **Full** | |
| `expander` | `Callable[[str], List[str]]`, default `None` | — | **None** | Morphological expansion (e.g., `noun_forms_expander`) requires Vabamorf |

---

## 3. RegexTagger — Parameter-by-Parameter Comparison

| Parameter | EstNLTK | estnltk-rs | Coverage | Notes |
|-----------|---------|------------|----------|-------|
| `ruleset` / `patterns` | `Ruleset` object with `StaticExtractionRule` list | List of pattern dicts | **Full** | Different input format, same information carried |
| `output_layer` | str, default `'regexes'` | str, default `"regexes"` | **Full** | |
| `output_attributes` | Sequence, default `None` | `Vec<String>`, default auto-collected | **Full** | |
| `conflict_resolver` | str or callable | str only | **Partial** | Custom callable resolvers not supported |
| `overlapped` | bool, default `False` | bool, default `false` | **Full** | Iterative re-search from `start+1` after each match to find overlaps |
| `lowercase_text` | bool, default `False` | bool, default `false` | **Full** | |
| `decorator` (global) | `Callable[[Text, ElementaryBaseSpan, Dict], Optional[Dict]]` | — | **None** | Python callables can't run in Rust |
| `match_attribute` | str, default `'match'`; stores `re.Match` object | `Option<String>`, default `None`; stores matched text substring | **Partial** | Rust stores the matched text `String` instead of a `re.Match` object. Opt-in (default `None`), unlike EstNLTK which defaults to `'match'` |
| `group_attribute` | str, default `None` | `Option<String>`, default `None` | **Full** | |
| `priority_attribute` | str, default `None` | `Option<String>`, default `None` | **Full** | |
| `pattern_attribute` | str, default `None` | `Option<String>`, default `None` | **Full** | |

---

## 4. Extraction Rules

| Feature | EstNLTK | estnltk-rs | Coverage |
|---------|---------|------------|----------|
| StaticExtractionRule | Frozen dataclass: `pattern`, `attributes`, `group`, `priority` | `ExtractionRule` struct with same fields + compiled regex | **Full** |
| DynamicExtractionRule | Frozen dataclass: `pattern`, `decorator`, `group`, `priority` | — | **None** |
| Pattern type | Any (typically `regex.Regex` or string) | String compiled to `resharp::Regex` | **Partial** |
| `group` field | Any non-negative int (selects capture group) | Any non-negative int; two-pass extraction via anchored `regex` crate. Validated against pattern's capture count at construction | **Full** |
| `priority` field | int, default 0 | i32, default 0 | **Full** |
| `attributes` field | `Dict[str, Any]` — any Python value | `HashMap<String, AnnotationValue>` — str/int/float/bool/null only | **Partial** |

**Notes:**
- DynamicExtractionRule carries a Python callable (`decorator`) that modifies annotations at match time. This concept is inherently Python-specific.
- EstNLTK's `attributes` dict can hold arbitrary Python objects (lists, dicts, custom classes). The Rust side supports only scalar types.
- Capture group support uses a two-pass approach: resharp finds the full match (group 0), then an anchored `regex::Regex` (`^(?:<pattern>)$`) extracts the requested group from the matched substring. The anchoring eliminates leftmost-first vs leftmost-longest divergence between engines. Patterns using resharp-only syntax (intersection `&`, complement `~`) cannot use capture groups since the `regex` crate cannot parse them.

---

## 5. Rulesets

| Feature | EstNLTK | estnltk-rs | Coverage |
|---------|---------|------------|----------|
| `Ruleset` (unique patterns) | Enforces no duplicate patterns per rule type | `unique_patterns=true` rejects duplicate patterns at construction | **Full** |
| `AmbiguousRuleset` (multi-pattern) | Allows multiple rules per pattern | Default behavior — all rules applied | **Partial** |
| CSV loading (`ruleset.load()`) | Reads rules from CSV with typed columns (int, float, regex, string, callable, expression) | `rs_load_rules_csv` function with typed columns (int, float, string, bool) | **Partial** |
| `CONVERSION_MAP` type coercions | `int`, `float`, `regex`, `string`, `callable`, `expression` | `int`, `float`, `string`, `bool` | **Partial** |
| `rule_map` property | Maps patterns to grouped rules | — | **None** |
| `output_attributes` property | Computes attribute union from all rules | Auto-collected in `rs_regex_tag` convenience function | **Partial** |
| `missing_attributes` validation | Checks if rules have inconsistent attribute sets | `missing_attributes()` method on taggers + annotation normalization (fills missing attrs with `Null`) | **Full** |

---

## 6. Conflict Resolution

| Strategy | EstNLTK | estnltk-rs | Coverage |
|----------|---------|------------|----------|
| `KEEP_ALL` | Keep all matches | Keep all matches | **Full** |
| `KEEP_MAXIMAL` | Generator: remove spans covered by another | Vec-based: same algorithm | **Full** |
| `KEEP_MINIMAL` | Generator: remove spans enclosing another | Vec-based: same algorithm | **Full** |
| `KEEP_ALL_EXCEPT_PRIORITY` | Priority filter, then keep all | Priority filter, then keep all | **Full** |
| `KEEP_MAXIMAL_EXCEPT_PRIORITY` | Priority filter, then maximal | Priority filter, then maximal | **Full** |
| `KEEP_MINIMAL_EXCEPT_PRIORITY` | Priority filter, then minimal | Priority filter, then minimal | **Full** |
| Custom callable resolver | `conflict_resolver=my_function` | — | **None** |

**Algorithm fidelity:**
- `keep_maximal_matches`: Direct port. Verified identical output on overlapping spans from EstNLTK test suite ("Muna ja kana." with `m..a.ja`, `ja`, `ja.k..a`).
- `keep_minimal_matches`: Direct port of worklist algorithm. Same test verification.
- `conflict_priority_resolver`: O(n²) implementation matching Python behavior. Same overlap and group semantics.

---

## 7. Decorator / Annotation Pipeline

| Stage | EstNLTK | estnltk-rs | Coverage |
|-------|---------|------------|----------|
| 1. Static attributes from rule | Copied into annotation dict | Copied into annotation `HashMap` | **Full** |
| 2. `match` attribute added | `re.Match` object stored under `match_attribute` name | Matched text `String` stored under `match_attribute` name (when set) | **Partial** |
| 3. Global decorator applied | `decorator(text_obj, base_span, annotation_dict) -> dict or None` | — | **None** |
| 4. Dynamic decorator applied | Per `(group, priority)` key: `decorator(text_obj, base_span, annotation_dict)` | — | **None** |
| 5. Drop if decorator returns `None` | Span removed from output | — | **N/A** |
| 6. Optional group/priority/pattern attributes | Added if corresponding `*_attribute` param is set | Added if corresponding `*_attribute` param is set | **Full** |

**Notes:**
- Decorators are the primary extensibility mechanism in EstNLTK's rule taggers. They allow runtime modification of annotations based on context (surrounding text, morphological analysis, etc.).
- The Rust side produces static annotations only. Decorators can be applied Python-side on the Rust output as a post-processing step.
- `match_attribute`: EstNLTK stores a `re.Match` object (with `.group()`, `.span()`, etc.). The Rust equivalent stores the matched text `String` — sufficient for the most common use case (inspecting what was matched). When `group > 0`, the stored text is the capture group's text, not the full match. When `lowercase_text=true`, the stored text comes from the lowercased input.

---

## 8. Regex Engine Differences

| Feature | EstNLTK (`regex` library) | estnltk-rs (`resharp`) |
|---------|--------------------------|------------------------|
| Engine type | NFA backtracking | DFA (deterministic) |
| Time complexity | Exponential worst case | Guaranteed linear |
| Capture groups | Full support (numbered and named) | Numbered groups via two-pass extraction (resharp match + `regex` crate group extraction). Named groups not supported |
| Lazy quantifiers | `.*?`, `.+?`, etc. | Not supported |
| Overlapped matching | `finditer(overlapped=True)` | Supported via `overlapped=true` parameter (iterative re-search) |
| Matching semantics | Leftmost-first | Leftmost-longest |
| Lookaround | Limited (fixed-width lookbehind) | Native support |
| Boolean operations | Not supported | Intersection (`&`), complement (`~`) |
| Unicode | Full | Full (operates on bytes, converted to chars) |
| Match result | `re.Match` object with groups, span, string | `Match { start, end }` byte offsets only |

**Semantic differences that affect output:**
- Alternation: `cat|catfish` → Python matches `"cat"`, resharp matches `"catfish"` (leftmost-longest)
- Greedy vs. lazy: Python `a.*?b` matches shortest `a...b`; resharp has no equivalent (must use negated character class like `a[^b]*b`)
- Overlapping: Pattern `aa` on `"aaa"` → both find 2 matches with `overlapped=True` (Rust iteratively re-searches from `start+1`); without overlapping, resharp finds 1

---

## 9. Regex Library (Pattern Composition)

EstNLTK provides a `regex_library` subpackage for building regex patterns programmatically:

| Class | Purpose | estnltk-rs | Coverage |
|-------|---------|------------|----------|
| `RegexElement` | Base class: wraps pattern with test infrastructure | — | **None** |
| `RegexPattern` | Template: `pattern.format(sub=RegexElement(...))` | — | **None** |
| `ChoiceGroup` | Alternation of `RegexElement` children with test merging | — | **None** |
| `StringList` | Sorted string list → regex choice (longest first, case/replacement options) | `rs_string_list_pattern` function: longest-first sorting, regex escaping, `ignore_case` (`[Xx]` notation), per-string case flags, character replacement maps, deduplication | **Partial** |

**Features not ported:**
- Pattern validation with positive/negative/extraction test suites (`RegexElement` test infrastructure)
- Jupyter HTML display (`_repr_html_`)
- CSV/TXT file loading for string lists (patterns can be loaded via `rs_load_rules_csv` separately)
- Named capture group management
- `group_name` / `description` metadata fields
- `from_file()` / `to_csv()` static methods

**Features ported (StringList):**
- Auto-sorting strings by length (longest first) for greedy matching
- Character replacement maps (e.g., space → `\s+`) with non-capture group wrapping
- Global and per-string case sensitivity control (`ignore_case` / `ignore_case_flags`)
- Regex metacharacter escaping
- String deduplication (case-aware when `ignore_case` is set)
- Non-capture group wrapping of output pattern

**Notes:**
- These are development-time tools for building and testing regex patterns. They produce standard regex strings that can be passed to either engine.
- Since resharp accepts standard regex syntax (minus lazy quantifiers), patterns built with `regex_library` can often be used directly. Capture groups are supported via two-pass extraction.
- `StringList` is ported as a pure function (`rs_string_list_pattern`) rather than a class, since the Rust side does not need the test infrastructure or Jupyter display from `RegexElement`.

---

## 10. Helper Methods

| Function | EstNLTK | estnltk-rs | Coverage |
|----------|---------|------------|----------|
| `keep_maximal_matches` | Generator | `Vec`-returning function | **Full** |
| `keep_minimal_matches` | Generator | `Vec`-returning function | **Full** |
| `conflict_priority_resolver` | List-modifying function | `Vec`-returning function | **Full** |
| `noun_forms_expander` | Generates all 28 case forms (14 cases × sg/pl) using Vabamorf | — | **None** |
| `verb_forms_expander` | Not implemented (`raise NotImplementedError`) | — | **N/A** |
| `default_expander` | Calls `noun_forms_expander` | — | **None** |

**Notes:**
- Morphological expansion (`noun_forms_expander`) depends on Vabamorf, a C++ Estonian morphological analyzer with Python bindings. Not portable to Rust without FFI.
- The expander is used by `SubstringTagger`. The Rust `SubstringTagger` does not support expanders — patterns must be pre-expanded before passing to Rust.

---

## 11. Data Types and Layer Model

| Concept | EstNLTK | estnltk-rs | Coverage |
|---------|---------|------------|----------|
| `Text` object | Rich text container with layer management, morphological analysis | Plain `&str` input | **Partial** |
| `Layer` | Named, typed container for spans with parent/enveloping relationships | `TagResult` struct (flat dict output) | **Partial** |
| `Span` | Base span + annotations, linked to layer | `TaggedSpan` (span + annotations, no layer link) | **Partial** |
| `ElementaryBaseSpan` | `(start, end)` character offsets | `MatchSpan { start, end }` | **Full** |
| `EnvelopingBaseSpan` | Tuple of `ElementaryBaseSpan` (for PhraseTagger) | — | **None** |
| `Annotation` | Dict of attribute values, linked to span | `HashMap<String, AnnotationValue>` | **Partial** |
| `ambiguous` flag | Per-layer, controls whether multiple annotations per span allowed | Controlled by `ambiguous_output_layer` config (default `true`) | **Full** |
| `parent` relationship | Layer can declare parent layer | — | **None** |
| `enveloping` relationship | Layer can declare enveloping layer | — | **None** |
| `secondary_attributes` | Additional layer attributes | — | **None** |
| `meta` dict | Layer metadata | — | **None** |
| `serialisation_module` | Custom serialization | — | **None** |
| `layer_to_dict` / `dict_to_layer` | Bidirectional serialization | `TagResult.to_pydict()` (one-way, to-dict only) | **Partial** |

---

## 12. Output Format

EstNLTK's `layer_to_dict()` returns:

```python
{
    "name": "regexes",
    "attributes": ("type",),
    "ambiguous": True,
    "enveloping": None,        # not produced by estnltk-rs
    "meta": {},                # not produced by estnltk-rs
    "parent": None,            # not produced by estnltk-rs
    "secondary_attributes": (),# not produced by estnltk-rs
    "serialisation_module": None, # not produced by estnltk-rs
    "spans": [
        {
            "base_span": (11, 21),
            "annotations": [{"type": "email"}]
        }
    ]
}
```

estnltk-rs `RsRegexTagger.tag()` returns:

```python
{
    "name": "regexes",
    "attributes": ["type"],
    "ambiguous": True,
    "spans": [
        {
            "base_span": (11, 21),
            "annotations": [{"type": "email"}]
        }
    ]
}
```

**Differences:**
- `attributes` is a list in Rust, a tuple in EstNLTK
- Missing fields: `enveloping`, `meta`, `parent`, `secondary_attributes`, `serialisation_module`
- No `dict_to_layer` (cannot reconstruct EstNLTK Layer from Rust output without EstNLTK)

---

## 13. Error Handling

| Scenario | EstNLTK | estnltk-rs |
|----------|---------|------------|
| Invalid regex pattern | `regex.error` at `Regex()` construction | `PyValueError` at tagger construction |
| `group != 0` | Silently uses specified group | Two-pass extraction: resharp finds full match, `regex` crate extracts group. `PyValueError` if group index exceeds capture count or pattern uses resharp-only syntax |
| `overlapped = True` | Uses `regex.finditer(overlapped=True)` | Uses iterative re-search from `start+1` after each match |
| Invalid conflict_resolver string | `ValueError` at `_make_layer` time | `PyValueError` at construction time |
| Conflicting patterns in Ruleset | `ValueError` from `Ruleset.add_rules` | `PyValueError` when `unique_patterns=true` (default: duplicates allowed) |
| Missing `pattern` key in dict | N/A (uses `StaticExtractionRule` dataclass) | `PyKeyError` |
| Unsupported attribute value type | No restriction (any Python object) | `PyTypeError` (only str/int/float/bool/None) |
| Invalid Aho-Corasick patterns | N/A (ahocorasick library handles) | `PyValueError` at `RsSubstringTagger` construction |

---

## 14. Testing

| Test Area | EstNLTK | estnltk-rs |
|-----------|---------|------------|
| Conflict resolution unit tests | In `test_custom_conflict_resolver.py` (across all 4 taggers) | `tests/test_conflict.rs` (8 tests) + `src/conflict.rs` (14 unit tests) |
| Regex tagger integration | Implicit in conflict resolver tests | `tests/test_tagger.rs` (8 tests) + `src/tagger.rs` (42 unit tests) |
| Substring tagger integration | Separate test file | `tests/test_substring_tagger.rs` (12 tests) + `src/substring_tagger.rs` (22 unit tests) |
| Cross-implementation parity (regex) | — | `cross_tests/test_cross_impl.py` (23 tests) |
| Cross-implementation parity (substring) | — | `cross_tests/test_cross_substring.py` (14 tests) |
| Byte↔char conversion | — | `src/byte_char.rs` (4 unit tests) |
| CSV vocabulary loading | `regex_vocabulary.csv` test fixture | `tests/test_csv_loader.rs` (5 tests) + `src/csv_loader.rs` (10 unit tests) |
| Capture group extraction | Implicit (group parameter in rules) | 13 unit tests + 2 integration tests (basic, multibyte, mixed rules, error cases) |
| Overlapped matching | Implicit in existing test suite | 10 unit tests (basic, multibyte, capture groups, conflict resolution, attributes) |
| Match attribute | Implicit in existing test suite | 7 unit tests (basic, capture groups, multibyte, lowercase, with attributes, disabled, overlapped) |
| StringList pattern composition | `StringList` class tests | `src/string_list.rs` (16 unit tests) |
| Decorator chain tests | Various in existing test suite | — |
| Custom conflict resolver | `_conflict_resolver_keep_first` in test suite | — |
| SpanTagger tests | Separate test file | — |
| PhraseTagger tests | Separate test file | — |

---

## Summary

| Category | Full | Partial | None |
|----------|------|---------|------|
| Tagger types (4) | 0 | 2 | 2 |
| RegexTagger parameters (11) | 7 | 2 | 2 |
| SubstringTagger parameters (12) | 9 | 1 | 2 |
| Extraction rules (6 features) | 3 | 2 | 1 |
| Rulesets (7 features) | 2 | 4 | 1 |
| Conflict strategies (7) | 6 | 0 | 1 |
| Decorator pipeline (6 stages) | 2 | 1 | 2 (+1 N/A) |
| Helper functions (6) | 3 | 0 | 2 (+1 N/A) |
| Regex library classes (4) | 0 | 1 | 3 |
| Data model (13 concepts) | 2 | 5 | 6 |

**What works identically:** Core regex matching → conflict resolution → annotation assembly pipeline for static attributes, including capture group extraction (any group index). Two-pass capture group support: resharp finds the full match, an anchored `regex::Regex` extracts the requested group from the matched substring — preserving resharp's leftmost-longest semantics. Overlapped matching (`overlapped=true`): iteratively re-searches from `match.start + 1` after each match, finding all overlapping spans — matching Python's `regex.finditer(overlapped=True)` semantics. Substring matching with Aho-Corasick, token separator boundary checking, and all conflict strategies. CSV rule loading with typed columns (int, float, string, bool). Missing attribute validation and annotation normalization (missing attributes filled with `Null`). Ambiguous/non-ambiguous output layer control (`ambiguous_output_layer` parameter). Ruleset uniqueness enforcement (`unique_patterns` parameter — when `true`, rejects duplicate patterns matching EstNLTK's `Ruleset` semantics; default `false` matches `AmbiguousRuleset`). `StringList` pattern composition: longest-first sorting, regex escaping, case-insensitive conversion, character replacement maps, and deduplication — matching EstNLTK's `regex_library.StringList` core functionality. Verified by 37 cross-implementation tests (23 regex + 14 substring) including Estonian multi-byte text. 148 Rust tests total (115 unit + 33 integration).

**Biggest gaps:** Decorators (global and dynamic), other tagger types (Span/Phrase), regex library composition tools, morphological expanders.

**By design, not ported:** Features tied to Python runtime (decorators, arbitrary attribute types, callable conflict resolvers, morphological expanders) and EstNLTK's layer infrastructure (parent/enveloping relationships, `Text` object integration).

**Partial equivalents:** `match_attribute` stores the matched text `String` instead of Python's `re.Match` object — opt-in via `match_attribute` parameter (default `None`), covering the most common use case (inspecting matched text) without Python-specific match objects.
