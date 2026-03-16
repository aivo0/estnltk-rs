# Code Review: `estnltk-segmentation` Crate

## Overview

New crate implementing the full Estonian text segmentation pipeline (3,526 lines across 13 Rust files + 4 data files). Ports Python EstNLTK's 5-component pipeline: TokensTagger → CompoundTokenTagger → WordTagger → SentenceTokenizer → ParagraphTokenizer. All 31 tests pass, zero compiler warnings.

---

## Correctness Issues

### 1. Regex recompilation in hot path (performance + correctness risk)
- `compound_token/mod.rs:540-541` — `NumericWithPeriodNormalizer` compiles 2 regexes on every invocation of `apply_normalization`
- `compound_token/mod.rs:556-557` — `CompactPeriods` compiles 2 regexes per call
- `compound_token/mod.rs:563` — `CollapseWhitespace` compiles 1 regex per call
- `postcorrections.rs:37` — `fix_repeated_ending_punct` compiles `ending_punct_re` per call
- `paragraph_tagger.rs:27` — `detect_paragraphs` compiles `para_re` per call

These should be compiled once (e.g., stored in struct fields, `OnceLock`, or `lazy_static`). For compound tokens, this means normalization regexes could be called thousands of times on a large document.

### 2. `paragraph_tagger.rs:40` — `byte_to_char_map` recomputed inside the loop
```rust
for m in para_re.find_iter(text) {
    let b2c = estnltk_core::byte_to_char_map(text); // O(n) per match!
```
This is O(n*m) where m is the number of paragraph breaks. Should be hoisted outside the loop.

### 3. Punkt algorithm simplification may diverge from NLTK
- `punkt.rs:148-152` — Ellipsis detection checks `token.text.len() >= 2` using byte length, but `..` is 2 bytes while `…` (U+2026, single ellipsis char) is 3 bytes but 1 char. The Python code checks for 2+ consecutive period *characters*, not bytes. Should use `token.text.chars().count()`.
- `punkt.rs:224-229` — Empty block with comment "Actually in NLTK, lowercase after period usually keeps the sentbreak" — this is a stub that may cause divergence on edge cases.

### 4. `compound_token/mod.rs:182` — Group extraction uses wrong text slice
```rust
if let Some(caps) = pattern.regex.captures(&text[byte_start..]) {
```
This re-runs the regex on a substring starting at `byte_start`, not the original match. If the regex has anchors or context-dependent behavior, the capture groups could shift. The correct approach is to use `pattern.regex.captures_at(text, byte_start)` or run captures on the full text and filter.

### 5. `compound_token/mod.rs:263` — Dead code path for negative patterns
```rust
if covered.len() >= 2 || (covered.len() == 1 && pattern_type.starts_with("negative:"))
```
Negative patterns are already skipped at line 173 (`if pattern.is_negative { continue; }`), so the `negative:` branch here is unreachable.

### 6. `compound_token/mod.rs:237` — Clippy lint
```rust
if covered.len() < 1 {  // should be: if covered.is_empty() {
```

---

## Design Issues

### 7. `SegmentationError` is defined but never returned
`lib.rs:23-29` defines `SegmentationError` but `SegmentationPipeline::estonian()` returns `Self` (not `Result<Self, _>`) and `segment()` returns `SegmentationResult` directly. All regex compilation uses `.unwrap()`. Either make constructors return `Result` or remove the error type.

### 8. Convenience functions allocate taggers per call
`lib.rs:100-101`:
```rust
pub fn tokenize(text: &str) -> Vec<MatchSpan> {
    TokensTagger::new().tokenize(text) // compiles ~4 regexes every call
}
```
Same for `detect_compound_tokens` (compiles 40+ regexes) and `split_sentences` (compiles 30+ merge patterns + loads punkt params). These should document the cost or use `OnceLock` internally.

### 9. `postcorrections.rs:267` — `let _ = prev_fixes;` smell
This was added to silence an unused-variable warning, but `prev_fixes` was part of the original Python logic. In the merge-and-split path, the previous sentence's fix types should be propagated. Currently `prev_fixes` from the `new_fixes` path is never used for the `shift_ending` branch — the code clones it separately. This is correct but the `let _ = prev_fixes` suggests the binding isn't doing what was intended.

### 10. Hyphenation state machine uses string-typed states
`compound_token/mod.rs:289-317` uses `Option<&str>` with values `"-"`, `"second"`, `"end"` as state. An enum would be clearer and prevent typo bugs:
```rust
enum HyphenState { None, Hyphen, Second, End }
```

---

## Test Coverage Gaps

### 11. Missing cross-implementation tests
The plan calls for "cross-implementation tests" comparing Python and Rust output span-by-span. No such tests exist yet. The current tests are basic smoke tests — they verify that *something* is found but don't verify *exact spans*.

### 12. No tests for:
- Email/URL/emoticon compound token detection
- Sentence merge pattern behavior (only tests pattern compilation, not matching logic)
- Postcorrection steps (A through F individually)
- Edge cases: very long texts, Unicode edge cases (combining marks, ZWJ), texts with only punctuation
- Level 2 (non-strict) compound token matching

### 13. Weak assertion style
Many tests use `assert!(x.len() >= 1)` or `assert!(x.len() >= 2)` instead of exact expected values. This allows silent regressions where the count changes.

---

## Style / Convention Issues

### 14. Inconsistent `Option::None` vs `None`
Multiple files use `Option::None` instead of the idiomatic `None`:
- `compound_token/mod.rs:524,537,580`
- `postcorrections.rs:385`

### 15. `merge_patterns.rs:32` — HYPHEN_PAT contains duplicate `-`
```
r"(-|\u{2212}|...|\u{002D}|...|-)"
```
`\u{002D}` is the same as `-`, and the pattern has a plain `-` at both the start and end. This is harmless but noisy.

### 16. Missing `#[must_use]` on pure functions
Functions like `tokenize()`, `detect_compound_tokens()`, `assemble_words()`, `detect_paragraphs()` return values that are meaningless if discarded.

---

## Performance Notes

### 17. Quadratic token search in `apply_level1`
`compound_token/mod.rs:232-236` scans all tokens for every hint:
```rust
let covered: Vec<MatchSpan> = tokens
    .iter()
    .filter(|t| t.start >= span.start && t.end <= span.end)
    .copied()
    .collect();
```
With T tokens and H hints, this is O(T*H). Since both are sorted, a two-pointer or binary search approach would be O(T+H).

### 18. `spans_to_split.contains(&span)` in `split_into_symbols` is O(n)
`tokens_tagger.rs:146` — Linear search in a Vec. For many spans to split, this is quadratic. A `HashSet` would be O(1).

---

## Summary

| Category | Count |
|---|---|
| Correctness bugs | 4 (regex recompilation, b2c in loop, punkt byte-vs-char, group extraction) |
| Dead/unreachable code | 2 |
| Performance issues | 3 (regex recompilation, quadratic searches, b2c in loop) |
| Style/convention | 4 |
| Test coverage gaps | Significant — only smoke tests, no cross-validation |

**Overall**: Solid architectural port with good module structure and faithful mapping of the Python components. The main risks are (a) regex recompilation in hot paths degrading throughput, (b) Punkt algorithm simplifications causing divergence on real Estonian text, and (c) insufficient test coverage to catch span-level differences from the Python implementation. The code compiles cleanly and the existing tests pass, making this a strong foundation to iterate on.
