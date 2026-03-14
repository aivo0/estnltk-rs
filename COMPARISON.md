# EstNLTK vs estnltk-rs: Feature Comparison

Thorough comparison of EstNLTK's `rule_taggers` subsystem with the Rust rewrite in `estnltk-rs`.

Coverage legend: **Full** | **Partial** | **None** | **N/A** (not applicable to Rust)

---

## 1. Tagger Types

| Tagger | EstNLTK | estnltk-rs | Coverage |
|--------|---------|------------|----------|
| RegexTagger | `regex` library, supports capture groups, overlapping | `resharp` DFA engine, group=0 only | **Partial** |
| SubstringTagger | Aho-Corasick automaton for multi-string matching | `aho-corasick` automaton, token separators, static rules | **Partial** |
| SpanTagger | Matches input layer attribute values against ruleset | — | **None** |
| PhraseTagger | Matches sequential attribute values (phrase tuples), enveloping layer | — | **None** |

**Notes:**
- RegexTagger is the only tagger ported. The other three operate on existing layers rather than raw text, so they have different input requirements.
- SubstringTagger is ported with static rules, token separators, and all conflict strategies. Decorators and expander are not ported (Python-specific).
- SpanTagger and PhraseTagger depend on EstNLTK's layer/text infrastructure (`input_layer`, `input_attribute`), which has no Rust equivalent.

---

## 2. RegexTagger — Parameter-by-Parameter Comparison

| Parameter | EstNLTK | estnltk-rs | Coverage | Notes |
|-----------|---------|------------|----------|-------|
| `ruleset` / `patterns` | `Ruleset` object with `StaticExtractionRule` list | List of pattern dicts | **Full** | Different input format, same information carried |
| `output_layer` | str, default `'regexes'` | str, default `"regexes"` | **Full** | |
| `output_attributes` | Sequence, default `None` | `Vec<String>`, default auto-collected | **Full** | |
| `conflict_resolver` | str or callable | str only | **Partial** | Custom callable resolvers not supported |
| `overlapped` | bool, default `False` | Not supported; `True` rejected | **Partial** | resharp's `find_all` returns non-overlapping matches only |
| `lowercase_text` | bool, default `False` | bool, default `false` | **Full** | |
| `decorator` (global) | `Callable[[Text, ElementaryBaseSpan, Dict], Optional[Dict]]` | — | **None** | Python callables can't run in Rust |
| `match_attribute` | str, default `'match'`; stores `re.Match` object | — | **None** | resharp returns byte ranges, no `Match` object |
| `group_attribute` | str, default `None` | `Option<String>`, default `None` | **Full** | |
| `priority_attribute` | str, default `None` | `Option<String>`, default `None` | **Full** | |
| `pattern_attribute` | str, default `None` | `Option<String>`, default `None` | **Full** | |

---

## 3. Extraction Rules

| Feature | EstNLTK | estnltk-rs | Coverage |
|---------|---------|------------|----------|
| StaticExtractionRule | Frozen dataclass: `pattern`, `attributes`, `group`, `priority` | `ExtractionRule` struct with same fields + compiled regex | **Full** |
| DynamicExtractionRule | Frozen dataclass: `pattern`, `decorator`, `group`, `priority` | — | **None** |
| Pattern type | Any (typically `regex.Regex` or string) | String compiled to `resharp::Regex` | **Partial** |
| `group` field | Any non-negative int (selects capture group) | Must be 0; non-zero rejected at construction | **Partial** |
| `priority` field | int, default 0 | i32, default 0 | **Full** |
| `attributes` field | `Dict[str, Any]` — any Python value | `HashMap<String, AnnotationValue>` — str/int/float/bool/null only | **Partial** |

**Notes:**
- DynamicExtractionRule carries a Python callable (`decorator`) that modifies annotations at match time. This concept is inherently Python-specific.
- EstNLTK's `attributes` dict can hold arbitrary Python objects (lists, dicts, custom classes). The Rust side supports only scalar types.

---

## 4. Rulesets

| Feature | EstNLTK | estnltk-rs | Coverage |
|---------|---------|------------|----------|
| `Ruleset` (unique patterns) | Enforces no duplicate patterns per rule type | No validation — duplicate patterns allowed | **None** |
| `AmbiguousRuleset` (multi-pattern) | Allows multiple rules per pattern | Default behavior — all rules applied | **Partial** |
| CSV loading (`ruleset.load()`) | Reads rules from CSV with typed columns (int, float, regex, string, callable, expression) | — | **None** |
| `CONVERSION_MAP` type coercions | `int`, `float`, `regex`, `string`, `callable`, `expression` | — | **None** |
| `rule_map` property | Maps patterns to grouped rules | — | **None** |
| `output_attributes` property | Computes attribute union from all rules | Auto-collected in `rs_regex_tag` convenience function | **Partial** |
| `missing_attributes` validation | Checks if rules have inconsistent attribute sets | — | **None** |

---

## 5. Conflict Resolution

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

## 6. Decorator / Annotation Pipeline

| Stage | EstNLTK | estnltk-rs | Coverage |
|-------|---------|------------|----------|
| 1. Static attributes from rule | Copied into annotation dict | Copied into annotation `HashMap` | **Full** |
| 2. `match` attribute added | `re.Match` object stored under `match_attribute` name | — | **None** |
| 3. Global decorator applied | `decorator(text_obj, base_span, annotation_dict) -> dict or None` | — | **None** |
| 4. Dynamic decorator applied | Per `(group, priority)` key: `decorator(text_obj, base_span, annotation_dict)` | — | **None** |
| 5. Drop if decorator returns `None` | Span removed from output | — | **N/A** |
| 6. Optional group/priority/pattern attributes | Added if corresponding `*_attribute` param is set | Added if corresponding `*_attribute` param is set | **Full** |

**Notes:**
- Decorators are the primary extensibility mechanism in EstNLTK's rule taggers. They allow runtime modification of annotations based on context (surrounding text, morphological analysis, etc.).
- The Rust side produces static annotations only. Decorators can be applied Python-side on the Rust output as a post-processing step.

---

## 7. Regex Engine Differences

| Feature | EstNLTK (`regex` library) | estnltk-rs (`resharp`) |
|---------|--------------------------|------------------------|
| Engine type | NFA backtracking | DFA (deterministic) |
| Time complexity | Exponential worst case | Guaranteed linear |
| Capture groups | Full support (numbered and named) | Not supported |
| Lazy quantifiers | `.*?`, `.+?`, etc. | Not supported |
| Overlapped matching | `finditer(overlapped=True)` | Not supported (`find_all` is non-overlapping) |
| Matching semantics | Leftmost-first | Leftmost-longest |
| Lookaround | Limited (fixed-width lookbehind) | Native support |
| Boolean operations | Not supported | Intersection (`&`), complement (`~`) |
| Unicode | Full | Full (operates on bytes, converted to chars) |
| Match result | `re.Match` object with groups, span, string | `Match { start, end }` byte offsets only |

**Semantic differences that affect output:**
- Alternation: `cat|catfish` → Python matches `"cat"`, resharp matches `"catfish"` (leftmost-longest)
- Greedy vs. lazy: Python `a.*?b` matches shortest `a...b`; resharp has no equivalent (must use negated character class like `a[^b]*b`)
- Overlapping: Pattern `aa` on `"aaa"` → Python with `overlapped=True` finds 2 matches; resharp finds 1

---

## 8. Regex Library (Pattern Composition)

EstNLTK provides a `regex_library` subpackage for building regex patterns programmatically:

| Class | Purpose | estnltk-rs | Coverage |
|-------|---------|------------|----------|
| `RegexElement` | Base class: wraps pattern with test infrastructure | — | **None** |
| `RegexPattern` | Template: `pattern.format(sub=RegexElement(...))` | — | **None** |
| `ChoiceGroup` | Alternation of `RegexElement` children with test merging | — | **None** |
| `StringList` | Sorted string list → regex choice (longest first, case/replacement options) | — | **None** |

**Features not ported:**
- Pattern validation with positive/negative/extraction test suites
- Jupyter HTML display (`_repr_html_`)
- Auto-sorting strings by length for greedy matching
- Character replacement maps (e.g., space → `\s+`)
- Per-string case sensitivity control
- CSV/TXT file loading for string lists
- Named capture group management

**Notes:**
- These are development-time tools for building and testing regex patterns. They produce standard regex strings that can be passed to either engine.
- Since resharp accepts standard regex syntax (minus capture groups and lazy quantifiers), patterns built with `regex_library` can often be used directly — just remove capture groups.

---

## 9. Helper Methods

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
- The expander is only used by `SubstringTagger`, which is not ported.

---

## 10. Data Types and Layer Model

| Concept | EstNLTK | estnltk-rs | Coverage |
|---------|---------|------------|----------|
| `Text` object | Rich text container with layer management, morphological analysis | Plain `&str` input | **Partial** |
| `Layer` | Named, typed container for spans with parent/enveloping relationships | `TagResult` struct (flat dict output) | **Partial** |
| `Span` | Base span + annotations, linked to layer | `TaggedSpan` (span + annotations, no layer link) | **Partial** |
| `ElementaryBaseSpan` | `(start, end)` character offsets | `MatchSpan { start, end }` | **Full** |
| `EnvelopingBaseSpan` | Tuple of `ElementaryBaseSpan` (for PhraseTagger) | — | **None** |
| `Annotation` | Dict of attribute values, linked to span | `HashMap<String, AnnotationValue>` | **Partial** |
| `ambiguous` flag | Per-layer, controls whether multiple annotations per span allowed | Always `true` | **Partial** |
| `parent` relationship | Layer can declare parent layer | — | **None** |
| `enveloping` relationship | Layer can declare enveloping layer | — | **None** |
| `secondary_attributes` | Additional layer attributes | — | **None** |
| `meta` dict | Layer metadata | — | **None** |
| `serialisation_module` | Custom serialization | — | **None** |
| `layer_to_dict` / `dict_to_layer` | Bidirectional serialization | `TagResult.to_pydict()` (one-way, to-dict only) | **Partial** |

---

## 11. Output Format

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

## 12. Error Handling

| Scenario | EstNLTK | estnltk-rs |
|----------|---------|------------|
| Invalid regex pattern | `regex.error` at `Regex()` construction | `PyValueError` at tagger construction |
| `group != 0` | Silently uses specified group | `PyValueError` — rejected at construction |
| `overlapped = True` | Uses `regex.finditer(overlapped=True)` | Not supported (no parameter; resharp returns non-overlapping) |
| Invalid conflict_resolver string | `ValueError` at `_make_layer` time | `PyValueError` at construction time |
| Conflicting patterns in Ruleset | `ValueError` from `Ruleset.add_rules` | No validation (duplicates silently allowed) |
| Missing `pattern` key in dict | N/A (uses `StaticExtractionRule` dataclass) | `PyKeyError` |
| Unsupported attribute value type | No restriction (any Python object) | `PyTypeError` (only str/int/float/bool/None) |

---

## 13. Testing

| Test Area | EstNLTK | estnltk-rs |
|-----------|---------|------------|
| Conflict resolution unit tests | In `test_custom_conflict_resolver.py` (across all 4 taggers) | `tests/test_conflict.rs` (8 tests) + `src/conflict.rs` (10 unit tests) |
| Regex tagger integration | Implicit in conflict resolver tests | `tests/test_tagger.rs` (6 tests) + `src/tagger.rs` (8 unit tests) |
| Cross-implementation parity | — | `cross_tests/test_cross_impl.py` (23 tests), `cross_tests/test_cross_substring.py` (14 tests) |
| Byte↔char conversion | — | `src/byte_char.rs` (4 unit tests) |
| CSV vocabulary loading | `regex_vocabulary.csv` test fixture | — |
| Decorator chain tests | Various in existing test suite | — |
| Custom conflict resolver | `_conflict_resolver_keep_first` in test suite | — |
| SpanTagger tests | Separate test file | — |
| PhraseTagger tests | Separate test file | — |
| SubstringTagger tests | Separate test file | — |

---

## Summary

| Category | Full | Partial | None |
|----------|------|---------|------|
| Tagger types (4) | 0 | 2 | 2 |
| RegexTagger parameters (11) | 6 | 2 | 3 |
| Extraction rules (6 features) | 2 | 2 | 2 |
| Rulesets (7 features) | 0 | 2 | 5 |
| Conflict strategies (7) | 6 | 0 | 1 |
| Decorator pipeline (6 stages) | 2 | 0 | 3 (+1 N/A) |
| Helper functions (5) | 3 | 0 | 1 (+1 N/A) |
| Regex library classes (4) | 0 | 0 | 4 |
| Data model (12 concepts) | 1 | 5 | 6 |

**What works identically:** Core regex matching → conflict resolution → annotation assembly pipeline for group=0 patterns with static attributes. Verified by 23 cross-implementation tests including Estonian multi-byte text.

**Biggest gaps:** Decorators (global and dynamic), capture groups, overlapped matching, other tagger types (Substring/Span/Phrase), ruleset validation and CSV loading, regex library composition tools.

**By design, not ported:** Features tied to Python runtime (decorators, `re.Match` objects, arbitrary attribute types, callable conflict resolvers) and EstNLTK's layer infrastructure (parent/enveloping relationships, `Text` object integration).
