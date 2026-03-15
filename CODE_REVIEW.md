# Code Review: estnltk-regex-rs

## Overview

A well-structured Rust port of EstNLTK's regex-based text tagging subsystems, exposed to Python via PyO3. The code is clean, well-documented, thoroughly tested, and clearly organized. The review below focuses specifically on patterns that were ported too literally from Python and could be improved with more idiomatic Rust.

---

## High-Impact Improvements

### 1. Unnecessary String allocation when not lowercasing
**Files:** `tagger.rs:32-36`, `substring_tagger.rs:122-126`

```rust
// Current: always allocates
let raw_text = if self.config.lowercase_text {
    text.to_lowercase()
} else {
    text.to_string()  // <-- unnecessary clone of input
};
```

In Python, `str` assignment is a reference copy (free). In Rust this allocates a full copy. Use `Cow<str>`:

```rust
use std::borrow::Cow;
let raw_text: Cow<str> = if self.config.lowercase_text {
    Cow::Owned(text.to_lowercase())
} else {
    Cow::Borrowed(text)
};
```

This eliminates an allocation on every `tag()` call when `lowercase_text` is false (the common case).

### 2. O(n^2) conflict priority resolver — ported verbatim from Python
**File:** `conflict.rs:116-158`

```rust
// Comment says: "O(n^2) matching Python behavior"
for i in 0..n {
    for j in 0..n {  // <-- full cartesian product
```

Python's `O(n^2)` was acceptable due to typically small `n` and GIL overhead making algorithmic improvements less impactful. In Rust, this can be improved to `O(n log n)` with a sweep-line approach:
- Sort entries by start position
- Use an interval tree or active-set to find overlapping entries
- Eliminate by priority within groups

**Recommendation:** Keep the O(n^2) only if `n` is reliably small (< 100). If rulesets can grow large, this is the single biggest performance bottleneck after the regex engine itself. At minimum, the inner loop could start from `i+1` and handle both directions, halving the work.

### 3. Error types are all `String` — Python exception porting pattern
**Files:** All source files

Every function returns `Result<T, String>`. This is a direct port of Python's `raise ValueError("message")` pattern.

Idiomatic Rust would define a proper error enum:

```rust
#[derive(Debug, thiserror::Error)]
pub enum TaggerError {
    #[error("Duplicate pattern '{0}' not allowed with unique_patterns=true")]
    DuplicatePattern(String),
    #[error("Unknown conflict resolver: '{0}'")]
    UnknownStrategy(String),
    #[error("Regex compilation error: {0}")]
    RegexError(String),
    // ...
}
```

**Pragmatic note:** Since all errors cross the PyO3 boundary as Python exceptions anyway, `String` errors are functional. This is a code quality improvement, not a bug fix. Consider for a future refactor if the Rust API is used directly (not just via PyO3).

### 4. `matched_text` extraction uses O(n) char iteration
**File:** `tagger.rs:217-221`

```rust
let matched_text: String = text
    .chars()
    .skip(match_span.start)      // O(start)
    .take(match_span.end - match_span.start)  // O(len)
    .collect();
```

This is a direct port of Python's `text[start:end]` (which is O(1) for Python strings). In Rust, since you already have `byte_to_char_map`, you could build a reverse `char_to_byte_map` or store byte offsets alongside char offsets to do `&text[byte_start..byte_end]` in O(1).

**Fix:** Add byte offsets to `MatchSpan` or compute them during match extraction:
```rust
// If you stored byte offsets:
let matched_text = &text[byte_start..byte_end];
```

---

## Medium-Impact Improvements

### 5. `ConflictStrategy::from_str` should implement `std::str::FromStr`
**File:** `types.rs:220-231`

Custom `from_str` method instead of the standard trait. This prevents using `.parse::<ConflictStrategy>()` and other trait-based patterns.

```rust
impl std::str::FromStr for ConflictStrategy {
    type Err = String;  // or TaggerError
    fn from_str(s: &str) -> Result<Self, Self::Err> { ... }
}
```

Similarly, `ColumnType::from_str` in `csv_loader.rs:31-43`.

### 6. `Annotation` newtype has exposed inner HashMap via `.0`
**File:** `types.rs:94`

```rust
pub struct Annotation(pub HashMap<String, AnnotationValue>);
// Accessed everywhere as: annotation.0.insert(...), annotation.0.get(...)
```

This is a direct port of Python's dict. Consider implementing `Deref<Target=HashMap<...>>` or adding methods like `get()`, `insert()`, `iter()` to provide a cleaner API while keeping the inner field private.

### 7. `check_unique_patterns` clones owned values unnecessarily
**File:** `types.rs:291-303`

```rust
if !seen.insert(key.clone()) {  // <-- key is already owned, clone is wasteful
    return Err(format!("Duplicate pattern '{}'", key));
}
```

Since `key` is a `String` (already owned from `to_lowercase()` or `to_string()`), the error message can format before inserting:

```rust
if seen.contains(&key) {
    return Err(format!("Duplicate pattern '{}' ...", key));
}
seen.insert(key);
```

Same pattern in `check_unique_phrase_patterns` at types.rs:428.

### 8. `Vec<char>` collection for indexed iteration
**Files:** `string_list.rs:45`, `string_list.rs:361`

```rust
let chars: Vec<char> = pattern.chars().collect();  // Python-style indexing
let mut i = 0;
while i < chars.len() { ... chars[i] ... }
```

This allocates a Vec just to index by position. Use `char_indices()` with a peekable iterator or byte-based scanning:

```rust
let mut chars = pattern.char_indices().peekable();
while let Some((pos, ch)) = chars.next() {
    if ch == '\\' {
        if let Some(&(_, next_ch)) = chars.peek() {
            result.push('\\');
            result.push(next_ch);
            chars.next();
            continue;
        }
    }
    // ...
}
```

### 9. `keep_minimal_matches` allocates new Vecs each iteration
**File:** `conflict.rs:70-108`

```rust
for &current in sorted {
    let mut new_work_list = Vec::new();  // <-- allocation per element
    // ...
    work_list = new_work_list;  // <-- drops old, assigns new
}
```

Use double-buffering with `std::mem::swap`:

```rust
let mut work_a = Vec::new();
let mut work_b = Vec::new();
for &current in sorted {
    work_b.clear();
    // ... fill work_b from work_a ...
    std::mem::swap(&mut work_a, &mut work_b);
}
```

### 10. `token_separators` uses `Vec<char>` with linear `.contains()`
**File:** `substring_tagger.rs:64, 176-177`

```rust
token_separators: Vec<char>,
// ...
if !self.token_separators.contains(&prev_char) { continue; }
```

For small sets (1-3 chars) this is fine. For robustness, consider a `HashSet<char>` or a 128-bit ASCII bitset:

```rust
struct CharSet([u128; 2]);  // covers all of ASCII + Latin-1
impl CharSet {
    fn contains(&self, ch: char) -> bool { ... }
}
```

Likely not measurable unless token_separators gets large, but more idiomatic.

---

## Minor / Style Observations

### 11. `regex_escape` re-implements `regex::escape()`
**File:** `string_list.rs:4-13`

The hand-rolled `regex_escape` includes `#`, `&`, `-`, `~` which aren't metacharacters in Rust's `regex` crate but are in Python's `regex` module (POSIX extended). Consider using `regex::escape()` unless the extra escaping is intentional for resharp compatibility.

### 12. Three separate config structs with heavy overlap
**Files:** `types.rs:TaggerConfig`, `span_tagger.rs:SpanTaggerConfig`, `phrase_tagger.rs:PhraseTaggerConfig`

Each has: `output_layer`, `output_attributes`, `conflict_strategy`, `group_attribute`, `priority_attribute`, `pattern_attribute`, `ambiguous_output_layer`, `unique_patterns`. This mirrors Python's class inheritance pattern where each Python class has overlapping `__init__` parameters.

In Rust, extract a shared `CommonConfig` struct that each tagger config embeds. Not critical but reduces duplication and makes API evolution easier.

### 13. `has_missing_attributes` collects into HashSet for every rule
**File:** `types.rs:269-281`

Could short-circuit by sorting keys and comparing sorted slices, avoiding HashSet allocation:

```rust
// Faster for small attribute sets:
let mut first_keys: Vec<&String> = rules_attrs[0].keys().collect();
first_keys.sort();
for attrs in &rules_attrs[1..] {
    let mut keys: Vec<&String> = attrs.keys().collect();
    keys.sort();
    if keys != first_keys { return true; }
}
```

### 14. `PhraseTagger` duplicates extraction logic between `extract_matches` and `extract_matches_from_py`
**File:** `phrase_tagger.rs`

The Python-dict extraction path reimplements the matching logic. Consider extracting the core matching into a shared function that takes an iterator of `(MatchSpan, &str)` pairs (value to match), with different frontends for Rust `TagResult` and Python `PyDict`.

---

## What's Done Well

- **byte_char.rs**: Elegant O(n) precomputation with O(1) lookups. Clean and correct.
- **conflict.rs**: Good use of `Cow` for zero-copy `KeepAll` path.
- **Test coverage**: Comprehensive, with Estonian multibyte tests throughout.
- **PyO3 bindings**: Clean separation between Rust core and Python wrapper.
- **`resolve_conflicts` generic dispatch**: Nice use of closure for group/priority lookup.
- **Aho-Corasick pattern deduplication in SubstringTagger**: Smart to build one automaton from unique patterns.
- **README.md and COMPARISON.md**: Excellent documentation of known differences.

---

## Prioritized Action Items

| Priority | Item | Status |
|----------|------|--------|
| P0 | Cow<str> for non-lowercased text (#1) | DONE |
| P1 | Store byte offsets for match_attribute (#4) | DONE — `char_to_byte_map` in `byte_char.rs` |
| P1 | Double-buffer keep_minimal_matches (#9) | DONE — `std::mem::swap` in `conflict.rs` |
| P2 | Implement FromStr trait (#5) | DONE — `ConflictStrategy`, `ColumnType` |
| P2 | Remove unnecessary .clone() in check_unique (#7) | DONE — contains-then-insert |
| P2 | Use char_indices instead of Vec<char> (#8) | DONE — peekable iterator in `string_list.rs` |
| P2 | Annotation Deref/DerefMut (#6) | DONE — private inner field, `Deref`/`DerefMut`/`From` |
| P3 | Typed error enum (#3) | DONE — `TaggerError` in `types.rs` |
| P3 | Improve O(n^2) priority resolver (#2) | DONE — early termination in `conflict.rs` |
| P3 | has_missing_attributes sorted comparison (#13) | DONE — sorted `Vec` instead of `HashSet` |
| -- | regex_escape intentional divergence (#11) | DOCUMENTED — comment in `string_list.rs` |
| P3 | Shared config struct (#12) | DEFERRED — low ROI vs volume of field access changes |
| -- | token_separators linear contains (#10) | DEFERRED — fine for typical 1-3 char sets |
| -- | PhraseTagger extraction duplication (#14) | N/A — `PhraseTagger` has no `tag_from_py` path |
