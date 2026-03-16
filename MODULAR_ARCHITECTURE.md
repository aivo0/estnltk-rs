# EstNLTK Rust Port: Modular Architecture Analysis

Based on analysis of 60+ practical tutorials in `estnltk-src/tutorials/` and the current
Rust port in `estnltk-regex-rs/`.

---

## 1. Use Case Classification (from Tutorials)

### Category A: Text Segmentation
Splitting raw text into structural units.

| Use Case | Components | Standalone Value |
|----------|-----------|-----------------|
| Tokenization | TokensTagger, TokenSplitter | Yes — base for everything |
| Compound token detection | CompoundTokenTagger (emails, URLs, hashtags, abbreviations, emoticons, numbers) | Yes — preprocessing |
| Word normalization | WordTagger, normalized_form | Yes — cleaning |
| Sentence splitting | SentenceTokenizer + Estonian-specific post-corrections (abbreviations, emoticons, parentheses) | Yes — very common need |
| Paragraph detection | ParagraphTokenizer | Yes — document structure |
| Clause detection | ClauseTagger (requires morphology) | No — needs morph layer |

### Category B: Morphological Processing
Understanding word structure and forms.

| Use Case | Components | Standalone Value |
|----------|-----------|-----------------|
| Morphological analysis | VabamorfTagger → lemma, root, POS, form, ending, clitic | Yes — core NLP |
| Disambiguation | VabamorfDisambiguator, CorpusBasedMorphDisambiguator | Coupled to analysis |
| Morphological synthesis | MorphSynthesizer (generate inflected forms) | Yes — text generation |
| Spelling correction | SpellCheckRetagger | Yes — text cleaning |
| Syllabification | SyllabificationTagger | Yes — TTS, readability |
| Compound word detection | CompoundWordDetector | Yes — linguistic analysis |
| User dictionaries | Custom vocabulary integration | Extension point |
| Tag set conversion | GT categories, UD categories | Coupled to analysis |

### Category C: Rule-based Pattern Extraction
Finding patterns and entities using rules/regexes.

| Use Case | Components | Standalone Value |
|----------|-----------|-----------------|
| Regex pattern matching | RegexTagger + conflict resolution | Yes — general purpose |
| Multi-string search | SubstringTagger (Aho-Corasick) | Yes — general purpose |
| Attribute-based tagging | SpanTagger (post-process layer output) | Yes — pipeline chaining |
| Phrase/collocation matching | PhraseTagger (word tuple sequences) | Yes — general purpose |
| Vocabulary/dictionary tagging | VocabularyTagger | Yes — terminology marking |
| Grammar-based extraction | FiniteGrammarTagger | Yes — structured IE |

### Category D: Information Extraction
Extracting structured information from text.

| Use Case | Components | Standalone Value |
|----------|-----------|-----------------|
| Named Entity Recognition | NerTagger (PER, LOC, ORG), EstBERTNERTagger (11 types) | Yes — high demand |
| Address extraction | AddressPartTagger, AddressGrammarTagger (street, house, town, county, postal) | Yes — specific domain |
| Temporal expressions | TemporalExpressionTagger | Yes — event processing |
| Date recognition | DateTagger | Yes — medical/legal |
| Measurement tagging | MeasurementTagger (quantities + units) | Yes — scientific text |
| Verb chain detection | VerbChainDetector (mood, polarity, tense, voice) | Coupled to morph+clause |
| Noun phrase chunking | NounPhraseChunker | Coupled to morph |
| Adjective phrases | AdjectivePhraseTagger | Coupled to morph |

### Category E: Syntactic Analysis
Understanding sentence structure.

| Use Case | Components | Standalone Value |
|----------|-----------|-----------------|
| Syntax preprocessing | MorphExtendedTagger (8+ sub-taggers) | Coupled to morph |
| Dependency parsing | MaltParser, UDPipe, ViSL CG3, Stanza | External tools |
| Coreference resolution | CoreferenceTagger | Coupled to syntax |

### Category F: Infrastructure & Utilities
Supporting functionality.

| Use Case | Components | Standalone Value |
|----------|-----------|-----------------|
| Format conversion | CoNLL, JSON, TCF, Label Studio importers/exporters | Yes — interop |
| Layer operations | Split, join, flatten, merge, diff, gaps | Yes — toolkit |
| Readability scoring | FleschTagger (reading ease) | Yes — simple |
| Collocation networks | CollocNet | Yes — research |
| WordNet access | Estonian WordNet API | Yes — lexical DB |
| Corpus storage | PostgreSQL backend | Yes — large scale |
| Embeddings | BERT, Word2Vec | Neural — separate |

---

## 2. Distinct Functional Pieces

These are the atomic functional units identified across all tutorials, grouped by
their technical characteristics.

### Pure Algorithms (no I/O, no external deps)
1. **Conflict resolution** — keep_maximal, keep_minimal, priority-based (6 strategies)
2. **Byte-char offset mapping** — UTF-8 aware index conversion
3. **Annotation assembly** — building TagResult from matches + rule metadata
4. **Pattern composition** — StringList, ChoiceGroup, RegexPattern template expansion
5. **Readability scoring** — Flesch formula over syllable/word/sentence counts

### Regex/Matching Engines
6. **DFA regex matching** — resharp-based linear-time matching with capture groups
7. **Multi-string matching** — Aho-Corasick automaton for substring search
8. **Attribute matching** — exact/case-insensitive string match on annotation values
9. **Phrase sequence matching** — head-index algorithm for word tuple matching

### Data Loading
10. **CSV rule loading** — typed columns (string, int, float, bool, regex) with validation
11. **Rule validation** — resharp pattern compilation, duplicate detection

### Morphological Processing (C++ FFI)
12. **Morphological analysis** — Vabamorf analyze() with disambiguation
13. **Morphological synthesis** — generate word forms from lemma + features
14. **Spelling correction** — Vabamorf spellcheck() with suggestions
15. **Syllabification** — Vabamorf syllabify()
16. **Form expansion** — generate all case forms (14 cases x sg/pl = 28 forms)

### Text Segmentation (not yet in Rust)
17. **Tokenization** — regex-based token splitting
18. **Compound token detection** — pattern rules for emails, URLs, numbers, etc.
19. **Word normalization** — normalized_form attribute filling
20. **Sentence splitting** — Punkt + Estonian post-corrections
21. **Paragraph splitting** — newline-based grouping

### Information Extraction (not yet in Rust)
22. **Address parsing** — grammar-based Estonian address extraction
23. **Temporal expression parsing** — date/time normalization
24. **Measurement parsing** — quantity + unit extraction
25. **NER** — rule-based or ML entity recognition

### Serialization/Interchange
26. **Layer ↔ Dict/JSON conversion** — TagResult serialization
27. **CoNLL format** — import/export
28. **Python bindings** — PyO3 type conversion layer

---

## 3. Proposed Rust Crate Workspace

### Architecture Overview

```
estnltk-rs/  (workspace root)
│
├── estnltk-core/           Foundation types + algorithms
├── estnltk-patterns/       Regex pattern composition
├── estnltk-csv/            CSV rule loading
├── estnltk-taggers/        4 rule-based taggers
├── estnltk-morph/          Morphological expansion bridge
├── vabamorf-sys/           Raw C++ FFI bindings (existing)
├── vabamorf-rs/            Safe Vabamorf wrapper (existing)
└── estnltk-python/         PyO3 bindings (cdylib)
```

### Dependency Graph

```
                     estnltk-core
                    /      |      \
                   /       |       \
    estnltk-patterns  estnltk-csv  estnltk-taggers
                   \       |       /       |
                    \      |      /   vabamorf-rs
                     \     |     /        |
                    estnltk-morph    vabamorf-sys
                          |
                    estnltk-python  (depends on all above)
```

All arrows point downward. No circular dependencies.

### Crate Details

#### `estnltk-core` — Foundation
**Deps:** `thiserror`, `serde`

Contains every type and utility needed by multiple crates:
- `MatchSpan`, `AnnotationValue`, `Annotation`, `TaggedSpan`, `TagResult`
- `ConflictStrategy` enum, `TaggerError`, `TaggerConfig`
- `TaggerRule` trait
- Conflict resolution algorithms (functional piece #1)
- Annotation assembly helpers (functional piece #3)
- Byte-char offset conversion (functional piece #2)

**Why separate:** Zero dependency on any regex engine, Aho-Corasick, CSV, or PyO3.
Anyone building a custom tagger depends only on this.

#### `estnltk-patterns` — Pattern Composition
**Deps:** `estnltk-core`, `resharp`, `regex`

- `StringList` — longest-first sorted regex alternation from word lists
- `ChoiceGroup` — `(?:pat1|pat2|...)` alternation builder
- `MergedStringLists` — optimized merge of compatible lists
- `RegexPattern` — `{name}` template substitution with resharp validation

**Why separate:** Useful standalone for building extraction patterns from vocabulary
lists. A user composing address/date/measurement patterns uses this directly without
needing any tagger.

#### `estnltk-csv` — CSV Rule Loading
**Deps:** `estnltk-core`, `csv`
**Optional feature:** `resharp-validation` (default on) — validates regex column type

- `load_rules_from_csv()` with typed columns
- `CsvLoadConfig`, `CsvRule`, `ColumnRef`

**Why separate:** Data loading is orthogonal to matching. Keeps `csv` dependency
out of the tagger crate for users who define rules programmatically.

#### `estnltk-taggers` — Rule-based Taggers
**Deps:** `estnltk-core`, `resharp`, `regex`, `aho-corasick`

All four taggers in one crate (they share config patterns, conflict pipeline,
annotation assembly):
- `RegexTagger` + `ExtractionRule` — DFA regex matching (functional piece #6)
- `SubstringTagger` + `SubstringRule` — Aho-Corasick matching (functional piece #7)
- `SpanTagger` + `SpanRule` — attribute value matching (functional piece #8)
- `PhraseTagger` + `PhraseRule` — word tuple matching (functional piece #9)

**Why one crate, not four:** These four taggers share the same conflict resolution
pipeline, TaggerConfig pattern, and annotation assembly logic. Nobody wants "just
RegexTagger without SubstringTagger" — they want the matching toolkit. Splitting
each into its own crate adds boilerplate with no practical benefit.

#### `estnltk-morph` — Morphological Expansion Bridge
**Deps:** `estnltk-core`, `estnltk-taggers` (for `SubstringRule`), `vabamorf-rs`

- `noun_forms_expander` — generates 28 forms (14 cases x sg/pl)
- `default_expander`
- `expand_rules()` — integrates expansion into SubstringTagger rules

**Why separate:** Bridges the heavyweight C++ FFI dependency (`vabamorf`) with the
taggers. Users who don't need morphological expansion skip the entire Vabamorf
compilation cost.

#### `vabamorf-sys` / `vabamorf-rs` — Already Isolated
No changes. Raw FFI + safe wrapper for Vabamorf C++ library.
- `analyze()`, `synthesize()`, `spellcheck()`, `syllabify()`

#### `estnltk-python` — Python Bindings
**Deps:** `pyo3`, all above crates
**Feature:** `vabamorf` (default) — enables morph expansion + PyVabamorf

The only crate with `crate-type = ["cdylib"]`. Split into per-tagger files:
- `py_regex_tagger.rs`, `py_substring_tagger.rs`, `py_span_tagger.rs`, `py_phrase_tagger.rs`
- `py_csv.rs`, `py_patterns.rs`, `py_vabamorf.rs`
- `py_types.rs` — `AnnotationValue::to_pyobject/from_pyobject` conversions

**Why separate:** Pure-Rust crates compile without PyO3, enabling Rust-native CLI
tools, WASM targets, or alternative language bindings (e.g., C, Node.js via uniffi).

### Minimum Useful Combinations

| Use Case | Crates Needed |
|----------|---------------|
| Build a custom tagger with own matching engine | `estnltk-core` |
| Compose regex patterns from word lists | `estnltk-core` + `estnltk-patterns` |
| Load extraction rules from CSV | `estnltk-core` + `estnltk-csv` |
| Run regex/substring matching on text | `estnltk-core` + `estnltk-taggers` |
| Full rule pipeline (CSV + patterns + taggers) | `core` + `patterns` + `csv` + `taggers` |
| Morphological analysis only | `vabamorf-sys` + `vabamorf-rs` |
| Taggers with morphological expansion | all above + `estnltk-morph` |
| Python interop | all above + `estnltk-python` |

---

## 4. Future Expansion Slots

The workspace is designed so new EstNLTK functionality maps to new crates without
restructuring existing ones:

| Future Crate | Purpose | Dependencies |
|-------------|---------|-------------|
| `estnltk-segmentation` | Tokenization, sentence/paragraph splitting | `estnltk-core` |
| `estnltk-grammar` | Finite grammar taggers | `estnltk-core`, `estnltk-taggers` |
| `estnltk-extractors` | Address, date, measurement, NER extractors | `estnltk-core`, `estnltk-taggers`, `estnltk-morph` |
| `estnltk-converters` | CoNLL, JSON, TCF import/export | `estnltk-core` |
| `estnltk-neural` | BERT/neural taggers (ONNX, candle) | `estnltk-core` |
| `estnltk-layers` | Layer operations (split, join, merge, diff) | `estnltk-core` |

---

## 5. Comparison to Python EstNLTK

| Aspect | Python EstNLTK (3 packages) | Proposed Rust (8+ crates) |
|--------|---------------------------|--------------------------|
| Granularity | `estnltk_core`, `estnltk`, `estnltk_neural` | 8 crates with precise dep boundaries |
| Vabamorf | Always bundled with `estnltk` | Optional crate chain, feature-gated |
| Pattern builders | Buried inside `rule_taggers/` subpackage | First-class standalone crate |
| Python coupling | Entire codebase is Python | Python bindings isolated; core is pure Rust |
| CSV loading | Mixed with tagger code | Separate crate, optional resharp validation |
| Conflict resolution | Helper file inside `rule_taggers/` | Part of core, reusable by any tagger |
| Install | All-or-nothing `pip install estnltk` | Selective `cargo add estnltk-taggers` |
| Non-Python use | Impossible | CLI tools, WASM, C FFI, other lang bindings |
| Compile cost | N/A | Users pay only for crates they use |

---

## 6. Key Refactoring Steps (Current Code)

To transform the current single-crate `estnltk-regex-rs` into the workspace:

1. **`types.rs`** — Split three ways:
   - Pure types → `estnltk-core`
   - `ExtractionRule` (contains `resharp::Regex`) → `estnltk-taggers`
   - PyO3 conversion methods → `estnltk-python/py_types.rs`

2. **`lib.rs`** (1329 lines of PyO3 glue) — Move entirely to `estnltk-python`, split into per-tagger files

3. **`conflict.rs`** — Move to `estnltk-core` (no changes needed beyond import paths)

4. **`string_list.rs`** — Move to `estnltk-patterns`, split into 4 files

5. **`csv_loader.rs`** — Move to `estnltk-csv`

6. **Tagger files** (`tagger.rs`, `substring_tagger.rs`, `span_tagger.rs`, `phrase_tagger.rs`) — Move to `estnltk-taggers`

7. **`expander.rs`** — Move to `estnltk-morph`

8. **`byte_char.rs`** — Move to `estnltk-core`

9. **`SpanTagger::tag_from_py`** — Move to `estnltk-python` (it takes `PyDict` directly)
