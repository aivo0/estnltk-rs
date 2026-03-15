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

## Performance: Rust vs Python

All benchmarks verify output parity — both implementations produce identical spans.

### RegexTagger (resharp DFA vs Python `regex`)

| Scenario | Text | Patterns | Python (ms) | Rust (ms) | Speedup |
|----------|------|----------|-------------|-----------|---------|
| small | 1 KB | 3 | 0.14 | 0.07 | **1.9x** |
| medium | 10 KB | 10 | 6.90 | 5.35 | **1.3x** |
| large | 100 KB | 50 | 169.86 | 74.69 | **2.3x** |

### SubstringTagger (Aho-Corasick vs Python `ahocorasick`)

| Scenario | Text | Patterns | Python (ms) | Rust (ms) | Speedup |
|----------|------|----------|-------------|-----------|---------|
| small | 1 KB | 10 | 0.25 | 0.14 | **1.9x** |
| medium | 10 KB | 50 | 5.13 | 1.40 | **3.7x** |
| large | 100 KB | 207 | 97.47 | 27.24 | **3.6x** |

### Rust-only Criterion benchmarks (10 KB text, 10 patterns)

| Benchmark | Time |
|-----------|------|
| RegexTagger tag | 96.6 µs |
| SubstringTagger tag | 42.3 µs |
| KEEP_ALL strategy | 104.2 µs |
| KEEP_MAXIMAL strategy | 107.8 µs |
| KEEP_MINIMAL strategy | 120.3 µs |
| lowercase=false | 99.2 µs |
| lowercase=true | 105.6 µs |
| keep_maximal (1000 spans) | 1.81 µs |
| keep_minimal (1000 spans) | 13.6 µs |
| priority_resolver (1000 spans) | 638.3 µs |

**Notes:**
- Speedup is end-to-end including PyO3 serialization overhead (Python dict construction on return). Pure Rust throughput is higher.
- The RegexTagger speedup is modest at medium scale because resharp DFA compilation is a one-time cost amortized over matching, and the Python `regex` library is itself a C extension. The gap widens at larger scales.
- SubstringTagger shows consistently higher speedup (3.6–3.7x) because the Rust Aho-Corasick implementation has lower per-match overhead than Python's.
- Benchmarks run with `cargo bench` (Criterion) and `python benchmarks/rust_vs_python/bench_*.py`.

## Limitations

- **No decorators** — Rust produces static annotations only. Decorators can be applied Python-side on the output.
- **Leftmost-longest semantics** — resharp uses leftmost-longest (not leftmost-first). `cat|catfish` matches "catfish" in resharp vs "cat" in Python `re`.
- **No lazy quantifiers** — `.*?` not supported. Use character class negation or lookahead instead.
