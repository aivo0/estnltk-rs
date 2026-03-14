# estnltk-rs

[EstNLTK](https://github.com/estnltk/estnltk) regex subsystems rewritten in Rust using the [resharp](https://crates.io/crates/resharp) DFA regex engine, exposed to Python via PyO3.

## Why

- **resharp** provides guaranteed linear-time matching, native lookaround, and boolean operations (intersection, complement)
- The RegexTagger is the most self-contained regex-dependent component in EstNLTK — pattern matching → conflict resolution → annotation assembly

## What's included

| Module | Purpose |
|--------|---------|
| `src/types.rs` | Core data types: spans, annotations, rules, config |
| `src/byte_char.rs` | UTF-8 byte↔char offset conversion (resharp returns byte offsets, EstNLTK uses char offsets) |
| `src/conflict.rs` | Conflict resolution: `keep_maximal`, `keep_minimal`, priority resolver |
| `src/tagger.rs` | RegexTagger core pipeline |
| `src/lib.rs` | PyO3 bindings: `RsRegexTagger` class and `rs_regex_tag()` function |

## Setup

```bash
# Install maturin
pip install maturin

# Build and install the Python module
cd estnltk-rs
maturin develop
```

## Python usage

```python
from estnltk_regex_rs import RsRegexTagger, rs_regex_tag

# Class-based API
tagger = RsRegexTagger(
    patterns=[
        {"pattern": r"[a-zA-Z0-9_.+-]+@[a-zA-Z0-9-]+\.[a-zA-Z0-9-.]+",
         "attributes": {"type": "email"}, "group": 0, "priority": 0},
    ],
    output_layer="regexes",
    output_attributes=["type"],
    conflict_resolver="KEEP_MAXIMAL",  # KEEP_ALL, KEEP_MINIMAL, *_EXCEPT_PRIORITY variants
    lowercase_text=False,
)
result = tagger.tag("Contact bla@bla.ee for info")
# {'name': 'regexes', 'attributes': ['type'], 'ambiguous': True,
#  'spans': [{'base_span': (8, 18), 'annotations': [{'type': 'email'}]}]}

# Convenience function
spans = rs_regex_tag("Hello 123", [{"pattern": r"[0-9]+", "attributes": {"type": "number"}}])
# [{'base_span': (6, 9), 'annotations': [{'type': 'number'}]}]
```

## Testing

```bash
# Rust unit + integration tests
cargo test

# Cross-implementation tests (requires estnltk installed)
cd cross_tests && pytest test_cross_impl.py -v
```

## Limitations

- **No capture groups** — only `group=0` (full match). Patterns using capture groups must be restructured.
- **No overlapped matching** — `overlapped=True` is rejected. Default `False` covers primary use cases.
- **No decorators** — Rust produces static annotations only. Decorators can be applied Python-side on the output.
- **Leftmost-longest semantics** — resharp uses leftmost-longest (not leftmost-first). `cat|catfish` matches "catfish" in resharp vs "cat" in Python `re`.
- **No lazy quantifiers** — `.*?` not supported. Use character class negation or lookahead instead.
