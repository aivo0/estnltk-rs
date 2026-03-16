use std::collections::HashMap;

use pyo3::prelude::*;

use estnltk_patterns::{
    build_choice_group_pattern, build_merged_string_lists_pattern, build_regex_pattern,
    build_string_list_pattern,
};

/// Build a regex alternation pattern from a list of literal strings.
///
/// Port of EstNLTK's `StringList` from the `regex_library` subpackage.
/// Produces a non-capture group pattern like `(?:longest|medium|short)`
/// with strings sorted by length (longest first) for greedy matching.
///
/// Args:
///     strings: List of literal strings to match.
///     replacements: Optional dict mapping single characters to regex patterns
///                   (e.g., `{" ": r"\s+"}` to allow flexible whitespace).
///     ignore_case: If True, convert all strings to case-insensitive form
///                  using `[Xx]` character class notation.
///     ignore_case_flags: Optional per-string list of bools overriding
///                        `ignore_case`. Must match length of `strings`.
///
/// Returns:
///     A regex pattern string like `(?:choice1|choice2|...)`.
#[pyfunction]
#[pyo3(signature = (strings, replacements=None, ignore_case=false, ignore_case_flags=None))]
pub fn rs_string_list_pattern(
    strings: Vec<String>,
    replacements: Option<HashMap<String, String>>,
    ignore_case: bool,
    ignore_case_flags: Option<Vec<bool>>,
) -> PyResult<String> {
    let repl = replacements.unwrap_or_default();
    let flags_ref = ignore_case_flags.as_deref();
    build_string_list_pattern(&strings, &repl, ignore_case, flags_ref)
        .map_err(PyErr::from)
}

/// Build a regex choice group (alternation) from multiple regex patterns.
///
/// Port of EstNLTK's `ChoiceGroup` from the `regex_library` subpackage.
/// Produces a non-capture group pattern like `(?:pattern1|pattern2|...)`.
/// Each pattern is validated as a compilable regex.
///
/// Args:
///     patterns: List of regex pattern strings to combine via alternation.
///
/// Returns:
///     A regex pattern string like `(?:pattern1|pattern2|...)`.
#[pyfunction]
pub fn rs_choice_group_pattern(patterns: Vec<String>) -> PyResult<String> {
    build_choice_group_pattern(&patterns)
        .map_err(PyErr::from)
}

/// Merge multiple string lists into a single choice group with longest-first sorting.
///
/// Port of EstNLTK's `ChoiceGroup` optimized merge for compatible `StringList` children.
/// When all sub-expressions in a `ChoiceGroup` are `StringList`-s with the same
/// character replacements, their strings are merged into a single list sorted by
/// length (longest first) to guarantee that the longest match is found first.
///
/// Args:
///     string_lists: List of string lists to merge (each is a list of literal strings).
///     replacements: Optional shared character-to-regex replacement map
///                   (e.g., `{" ": r"\s+"}` to allow flexible whitespace).
///                   Must be the same for all string lists.
///     ignore_case: If True, convert all strings to case-insensitive form
///                  using `[Xx]` character class notation.
///     ignore_case_flags_per_list: Optional per-list case sensitivity flags.
///                                 Each inner list must match the length of its
///                                 corresponding string list.
///
/// Returns:
///     A regex pattern string with all strings merged and sorted longest-first.
#[pyfunction]
#[pyo3(signature = (string_lists, replacements=None, ignore_case=false, ignore_case_flags_per_list=None))]
pub fn rs_merged_string_lists_pattern(
    string_lists: Vec<Vec<String>>,
    replacements: Option<HashMap<String, String>>,
    ignore_case: bool,
    ignore_case_flags_per_list: Option<Vec<Vec<bool>>>,
) -> PyResult<String> {
    let repl = replacements.unwrap_or_default();
    let flags_ref = ignore_case_flags_per_list.as_deref();
    build_merged_string_lists_pattern(&string_lists, &repl, ignore_case, flags_ref)
        .map_err(PyErr::from)
}

/// Build a regex pattern from a template with named placeholders.
///
/// Port of EstNLTK's `RegexPattern` from the `regex_library` subpackage.
/// The template uses `{name}` syntax for placeholders. Each placeholder is
/// replaced with the corresponding pattern from the `components` dict, wrapped
/// in a non-capture group `(?:...)` to prevent operator precedence issues.
///
/// Use `{{` and `}}` for literal braces in the template (e.g., `r"\d{{3}}"` -> `\d{3}`).
///
/// The final composed pattern is validated with resharp to ensure it compiles.
///
/// Args:
///     template: Template string with `{name}` placeholders
///               (e.g., `r"(?:{prefix}\s+)?{main}"`).
///     components: Dict mapping placeholder names to regex pattern strings.
///
/// Returns:
///     The composed regex pattern string.
///
/// Example (Python):
///
/// ```python
/// rs_regex_pattern(
///     r"(?:{prefix}\s+)?{main}",
///     {"prefix": "Mr|Mrs|Dr", "main": r"[A-Z][a-z]+"}
/// )
/// # "(?:(?:Mr|Mrs|Dr)\\s+)?(?:[A-Z][a-z]+)"
/// ```
#[pyfunction]
pub fn rs_regex_pattern(template: &str, components: HashMap<String, String>) -> PyResult<String> {
    build_regex_pattern(template, &components)
        .map_err(PyErr::from)
}
