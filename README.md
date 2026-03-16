# estnltk-rs

[EstNLTK](https://github.com/estnltk/estnltk) regex subsystems rewritten in Rust using the [resharp](https://crates.io/crates/resharp) DFA regex engine, exposed to Python via PyO3.

## Why

- **resharp** provides guaranteed linear-time matching, native lookaround, and boolean operations (intersection, complement)
- The RegexTagger is the most self-contained regex-dependent component in EstNLTK — pattern matching → conflict resolution → annotation assembly

## What's included

The project is a Cargo workspace of focused crates:

| Crate | Purpose |
|-------|---------|
| `estnltk-core` | Foundation types (spans, annotations, rules, config), conflict resolution, byte↔char offset conversion |
| `estnltk-patterns` | Regex pattern composition: `StringList`, `ChoiceGroup`, `RegexPattern` |
| `estnltk-csv` | CSV rule loading with typed columns (string, int, float, bool, regex) |
| `estnltk-taggers` | 4 rule-based taggers: `RegexTagger`, `SubstringTagger`, `SpanTagger`, `PhraseTagger` |
| `estnltk-morph` | Morphological rule expansion via Vabamorf |
| `estnltk-grammar` | Finite grammar tagger: bottom-up chart parsing with conflict resolution, SEQ/MSEQ support, decorators, validators |
| `estnltk-python` | PyO3 bindings: `RsRegexTagger`, `RsSubstringTagger`, `RsSpanTagger`, `RsPhraseTagger`, `RsVabamorf` |
| `vabamorf-rs` | Safe Rust wrapper around C++ Vabamorf (analysis, synthesis, spellcheck, syllabification) |
| `vabamorf-sys` | Raw FFI bindings to C++ Vabamorf |

Pure-Rust users can depend on individual crates (e.g., `estnltk-core` + `estnltk-taggers`) without PyO3. Python users get everything through the `estnltk-python` crate, which exposes the `estnltk_regex_rs` module.

## Setup

```bash
# Install maturin
pip install maturin

# Build and install the Python module
cd estnltk-regex-rs
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
# Rust unit + integration tests (all workspace crates)
cargo test --workspace

# Cross-implementation tests (requires estnltk installed)
pytest cross_tests/ -v
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

### Rust-only Criterion benchmarks

#### Taggers (10 KB text, 10 patterns)

| Benchmark | Time |
|-----------|------|
| RegexTagger tag | 112 µs |
| SubstringTagger tag | 55 µs |
| KEEP_ALL strategy | 113 µs |
| KEEP_MAXIMAL strategy | 109 µs |
| KEEP_MINIMAL strategy | 110 µs |
| lowercase=false | 107 µs |
| lowercase=true | 111 µs |
| SpanTagger (1000 spans, 10 rules) | 70 µs |
| PhraseTagger (1000 spans, 8 rules) | 249 µs |
| keep_maximal (1000 spans) | 1.85 µs |
| keep_minimal (1000 spans) | 5.03 µs |
| priority_resolver (1000 spans) | 198 µs |

#### Grammar tagger

| Benchmark | Time |
|-----------|------|
| Graph construction (20 spans) | 10.3 µs |
| Graph construction (100 spans) | 53 µs |
| Graph construction (500 spans) | 276 µs |
| Graph construction (2000 spans) | 1.10 ms |
| Parse (20 spans, 4 rules) | 34.9 µs |
| Parse (100 spans, 4 rules) | 181 µs |
| Parse (500 spans, 4 rules) | 919 µs |
| Parse (100 spans, 2 rules) | 104 µs |
| Parse (100 spans, 12 rules) | 469 µs |
| Parse depth 2 | 2.4 µs |
| Parse depth 4 | 3.7 µs |
| Parse depth 8 | 6.7 µs |
| Full pipeline (100 spans) | 186 µs |
| Full pipeline (500 spans) | 949 µs |
| Priority resolution overhead | +17% |
| Decorator overhead | +48% |

**Notes:**
- Speedup is end-to-end including PyO3 serialization overhead (Python dict construction on return). Pure Rust throughput is higher.
- The RegexTagger speedup is modest at medium scale because resharp DFA compilation is a one-time cost amortized over matching, and the Python `regex` library is itself a C extension. The gap widens at larger scales.
- SubstringTagger shows consistently higher speedup (3.6–3.7x) because the Rust Aho-Corasick implementation has lower per-match overhead than Python's.
- Benchmarks run with `cargo bench` (Criterion) and `python benchmarks/rust_vs_python/bench_*.py`.

### Vabamorf (Rust PyO3 vs Python SWIG — same C++ backend)

Both implementations wrap the same C++ Vabamorf library. The benchmark measures binding overhead (PyO3 vs SWIG) and data marshalling differences.

| Task | Input | Python (ms) | Rust (ms) | Speedup |
|------|-------|-------------|-----------|---------|
| analyze (disamb) | 10 words | 0.38 | 0.41 | 0.93x |
| analyze (disamb) | 72 words | 3.01 | 3.20 | 0.94x |
| analyze (disamb) | 264 words | 11.28 | 12.09 | 0.93x |
| analyze (no disamb) | 72 words | 1.58 | 1.43 | **1.11x** |
| synthesize | 20 calls | 0.23 | 0.25 | 0.94x |
| spellcheck | 72 words | 6.10 | 6.12 | 1.00x |

Performance is near-identical since both call the same C++ code. The Rust port's value is not speed but **integration**: morphological expansion feeds directly into SubstringTagger without crossing the Python boundary.

#### Vabamorf and morph expander

| Benchmark | Time |
|-----------|------|
| analyze disambiguated (6 words) | 260 µs |
| analyze raw (6 words) | 112 µs |
| analyze disambiguated (49 words) | 2.37 ms |
| analyze raw (49 words) | 1.09 ms |
| synthesize (5 calls) | 52.5 µs |
| spellcheck correct (6 words) | 30.8 µs |
| spellcheck suggest (6 words) | 6.76 ms |
| syllabify (49 words) | 25.2 µs |
| noun_forms_expander (8 nouns) | 2.06 ms |
| rule expansion (8 rules) | 2.02 ms |

## Limitations

- **Tagger decorators** — `RegexTagger`, `SubstringTagger`, `SpanTagger`, and `PhraseTagger` produce static annotations only. The grammar tagger supports decorators, validators, and scoring callbacks natively. For the other taggers, decorators can be applied Python-side on the output.
- **Grammar tagger Python bindings** — The `estnltk-grammar` crate is fully functional from Rust but not yet exposed via PyO3 bindings.
- **Leftmost-longest semantics** — resharp uses leftmost-longest (not leftmost-first). `cat|catfish` matches "catfish" in resharp vs "cat" in Python `re`.
- **No lazy quantifiers** — `.*?` not supported. Use character class negation or lookahead instead.
