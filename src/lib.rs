pub mod byte_char;
pub mod conflict;
pub mod csv_loader;
#[cfg(feature = "vabamorf")]
pub mod expander;
pub mod string_list;
pub mod substring_tagger;
pub mod tagger;
pub mod types;

use std::collections::HashMap;
#[cfg(feature = "vabamorf")]
use std::sync::Mutex;

use pyo3::prelude::*;
use pyo3::types::{PyDict, PyList};

use csv_loader::{ColumnRef, CsvLoadConfig, CsvRule};
use substring_tagger::{make_substring_rule, SubstringTagger};
use tagger::{make_rule, RegexTagger};
use types::*;

/// Parse a Python pattern dict into an ExtractionRule.
fn parse_pattern_dict(dict: &Bound<'_, PyDict>) -> PyResult<ExtractionRule> {
    let pattern: String = dict
        .get_item("pattern")?
        .ok_or_else(|| pyo3::exceptions::PyKeyError::new_err("'pattern' key required"))?
        .extract()?;

    let group: u32 = dict
        .get_item("group")?
        .map(|v| v.extract())
        .unwrap_or(Ok(0))?;

    let priority: i32 = dict
        .get_item("priority")?
        .map(|v| v.extract())
        .unwrap_or(Ok(0))?;

    let mut attributes = HashMap::new();
    if let Some(attrs_obj) = dict.get_item("attributes")? {
        if let Ok(attrs_dict) = attrs_obj.downcast::<PyDict>() {
            for (k, v) in attrs_dict.iter() {
                let key: String = k.extract()?;
                let val = AnnotationValue::from_pyobject(&v)?;
                attributes.insert(key, val);
            }
        }
    }

    make_rule(&pattern, attributes, group, priority)
        .map_err(|e| pyo3::exceptions::PyValueError::new_err(e))
}

/// Python-exposed RegexTagger class.
#[pyclass(name = "RsRegexTagger")]
struct PyRegexTagger {
    inner: RegexTagger,
}

#[pymethods]
impl PyRegexTagger {
    #[new]
    #[pyo3(signature = (patterns, output_layer="regexes", output_attributes=None, conflict_resolver="KEEP_MAXIMAL", lowercase_text=false, group_attribute=None, priority_attribute=None, pattern_attribute=None, ambiguous_output_layer=true, unique_patterns=false, overlapped=false, match_attribute=None))]
    fn new(
        patterns: &Bound<'_, PyList>,
        output_layer: &str,
        output_attributes: Option<Vec<String>>,
        conflict_resolver: &str,
        lowercase_text: bool,
        group_attribute: Option<String>,
        priority_attribute: Option<String>,
        pattern_attribute: Option<String>,
        ambiguous_output_layer: bool,
        unique_patterns: bool,
        overlapped: bool,
        match_attribute: Option<String>,
    ) -> PyResult<Self> {
        let mut rules = Vec::new();
        for item in patterns.iter() {
            let dict = item.downcast::<PyDict>().map_err(|_| {
                pyo3::exceptions::PyTypeError::new_err("Each pattern must be a dict")
            })?;
            rules.push(parse_pattern_dict(dict)?);
        }

        let strategy = ConflictStrategy::from_str(conflict_resolver)
            .map_err(|e| pyo3::exceptions::PyValueError::new_err(e))?;

        let config = TaggerConfig {
            output_layer: output_layer.to_string(),
            output_attributes: output_attributes.unwrap_or_default(),
            conflict_strategy: strategy,
            lowercase_text,
            group_attribute,
            priority_attribute,
            pattern_attribute,
            ambiguous_output_layer,
            unique_patterns,
            overlapped,
            match_attribute,
        };

        let tagger = RegexTagger::new(rules, config)
            .map_err(|e| pyo3::exceptions::PyValueError::new_err(e))?;

        Ok(Self { inner: tagger })
    }

    /// Tag text and return a layer dict.
    fn tag(&self, py: Python<'_>, text: &str) -> PyResult<PyObject> {
        let result = self.inner.tag(text);
        result.to_pydict(py)
    }

    /// Check if rules have inconsistent attribute sets.
    ///
    /// Returns True if some rules don't define the same set of attributes as others.
    /// Maps to EstNLTK's `AmbiguousRuleset.missing_attributes` property.
    #[getter]
    fn missing_attributes(&self) -> bool {
        self.inner.missing_attributes()
    }

    /// Return a dict mapping pattern strings to lists of rule dicts.
    ///
    /// Maps to EstNLTK's `Ruleset.rule_map` / `AmbiguousRuleset.rule_map` property.
    /// Each rule dict has keys: pattern, group, priority, attributes.
    #[getter]
    fn rule_map(&self, py: Python<'_>) -> PyResult<PyObject> {
        let map = self.inner.rule_map();
        let result = PyDict::new_bound(py);
        for (pattern, rule_indices) in &map {
            let rules_list = PyList::empty_bound(py);
            for &idx in rule_indices {
                let rule = &self.inner.rules[idx];
                let rule_dict = PyDict::new_bound(py);
                rule_dict.set_item("pattern", &rule.pattern_str)?;
                rule_dict.set_item("group", rule.group)?;
                rule_dict.set_item("priority", rule.priority)?;
                let attrs = PyDict::new_bound(py);
                for (k, v) in &rule.attributes {
                    attrs.set_item(k, v.to_pyobject(py))?;
                }
                rule_dict.set_item("attributes", attrs)?;
                rules_list.append(rule_dict)?;
            }
            result.set_item(pattern, rules_list)?;
        }
        Ok(result.unbind().into())
    }

    /// Return raw match spans as list of (start, end, rule_index) tuples.
    fn extract_matches(&self, py: Python<'_>, text: &str) -> PyResult<PyObject> {
        let raw_text = if self.inner.config.lowercase_text {
            text.to_lowercase()
        } else {
            text.to_string()
        };
        let b2c = byte_char::byte_to_char_map(&raw_text);
        let text_bytes = raw_text.as_bytes();
        let mut matches = Vec::new();

        for (rule_idx, rule) in self.inner.rules.iter().enumerate() {
            let found = rule
                .compiled
                .find_all(text_bytes)
                .map_err(|e| pyo3::exceptions::PyRuntimeError::new_err(format!("{}", e)))?;
            for m in found {
                let cs = b2c[m.start];
                let ce = b2c[m.end];
                if cs != ce {
                    matches.push((cs, ce, rule_idx));
                }
            }
        }
        matches.sort_by_key(|&(s, e, _)| (s, e));

        let list = PyList::empty_bound(py);
        for (s, e, ri) in matches {
            list.append((s, e, ri))?;
        }
        Ok(list.unbind().into())
    }
}

/// Convenience function: tag text with patterns and return list of span dicts.
#[pyfunction]
#[pyo3(signature = (text, patterns, conflict_resolver="KEEP_MAXIMAL", lowercase_text=false, ambiguous_output_layer=true, overlapped=false, match_attribute=None))]
fn rs_regex_tag(
    py: Python<'_>,
    text: &str,
    patterns: &Bound<'_, PyList>,
    conflict_resolver: &str,
    lowercase_text: bool,
    ambiguous_output_layer: bool,
    overlapped: bool,
    match_attribute: Option<String>,
) -> PyResult<PyObject> {
    let mut rules = Vec::new();
    let mut all_attr_names = Vec::new();
    for item in patterns.iter() {
        let dict = item.downcast::<PyDict>().map_err(|_| {
            pyo3::exceptions::PyTypeError::new_err("Each pattern must be a dict")
        })?;
        let rule = parse_pattern_dict(dict)?;
        for k in rule.attributes.keys() {
            if !all_attr_names.contains(k) {
                all_attr_names.push(k.clone());
            }
        }
        rules.push(rule);
    }

    let strategy = ConflictStrategy::from_str(conflict_resolver)
        .map_err(|e| pyo3::exceptions::PyValueError::new_err(e))?;

    let config = TaggerConfig {
        output_layer: "regexes".to_string(),
        output_attributes: all_attr_names,
        conflict_strategy: strategy,
        lowercase_text,
        group_attribute: None,
        priority_attribute: None,
        pattern_attribute: None,
        ambiguous_output_layer,
        unique_patterns: false,
        overlapped,
        match_attribute,
    };

    let tagger = RegexTagger::new(rules, config)
        .map_err(|e| pyo3::exceptions::PyValueError::new_err(e))?;

    let result = tagger.tag(text);

    let list = PyList::empty_bound(py);
    for tagged in &result.spans {
        let d = PyDict::new_bound(py);
        d.set_item("base_span", (tagged.span.start, tagged.span.end))?;
        let anns = PyList::empty_bound(py);
        for ann in &tagged.annotations {
            anns.append(ann.to_pydict(py)?)?;
        }
        d.set_item("annotations", anns)?;
        list.append(d)?;
    }
    Ok(list.unbind().into())
}

/// Parse a Python pattern dict into a SubstringRule.
fn parse_substring_pattern_dict(
    dict: &Bound<'_, PyDict>,
) -> PyResult<substring_tagger::SubstringRule> {
    let pattern: String = dict
        .get_item("pattern")?
        .ok_or_else(|| pyo3::exceptions::PyKeyError::new_err("'pattern' key required"))?
        .extract()?;

    let group: u32 = dict
        .get_item("group")?
        .map(|v| v.extract())
        .unwrap_or(Ok(0))?;

    let priority: i32 = dict
        .get_item("priority")?
        .map(|v| v.extract())
        .unwrap_or(Ok(0))?;

    let mut attributes = HashMap::new();
    if let Some(attrs_obj) = dict.get_item("attributes")? {
        if let Ok(attrs_dict) = attrs_obj.downcast::<PyDict>() {
            for (k, v) in attrs_dict.iter() {
                let key: String = k.extract()?;
                let val = AnnotationValue::from_pyobject(&v)?;
                attributes.insert(key, val);
            }
        }
    }

    Ok(make_substring_rule(&pattern, attributes, group, priority))
}

/// Python-exposed SubstringTagger class.
#[pyclass(name = "RsSubstringTagger")]
struct PySubstringTagger {
    inner: SubstringTagger,
}

#[pymethods]
impl PySubstringTagger {
    #[new]
    #[pyo3(signature = (patterns, output_layer="substrings", output_attributes=None, conflict_resolver="KEEP_MAXIMAL", lowercase_text=false, token_separators="", group_attribute=None, priority_attribute=None, pattern_attribute=None, ambiguous_output_layer=true, unique_patterns=false, expander=None, vabamorf=None))]
    fn new(
        patterns: &Bound<'_, PyList>,
        output_layer: &str,
        output_attributes: Option<Vec<String>>,
        conflict_resolver: &str,
        lowercase_text: bool,
        token_separators: &str,
        group_attribute: Option<String>,
        priority_attribute: Option<String>,
        pattern_attribute: Option<String>,
        ambiguous_output_layer: bool,
        unique_patterns: bool,
        expander: Option<&str>,
        vabamorf: Option<&Bound<'_, PyAny>>,
    ) -> PyResult<Self> {
        let mut rules = Vec::new();
        for item in patterns.iter() {
            let dict = item.downcast::<PyDict>().map_err(|_| {
                pyo3::exceptions::PyTypeError::new_err("Each pattern must be a dict")
            })?;
            rules.push(parse_substring_pattern_dict(dict)?);
        }

        // Apply expander if specified.
        let rules = if let Some(expander_name) = expander {
            #[cfg(feature = "vabamorf")]
            {
                let vm_obj = vabamorf.ok_or_else(|| {
                    pyo3::exceptions::PyValueError::new_err(
                        "expander requires a vabamorf parameter (RsVabamorf instance)",
                    )
                })?;
                let py_vm: &Bound<'_, PyVabamorf> = vm_obj.downcast().map_err(|_| {
                    pyo3::exceptions::PyTypeError::new_err(
                        "vabamorf must be an RsVabamorf instance",
                    )
                })?;
                let py_vm_ref = py_vm.borrow();
                let mut vm = py_vm_ref.inner.lock().unwrap();
                expander::expand_rules(rules, expander_name, &mut vm, lowercase_text)
                    .map_err(|e| pyo3::exceptions::PyValueError::new_err(e))?
            }
            #[cfg(not(feature = "vabamorf"))]
            {
                let _ = vabamorf;
                let _ = expander_name;
                return Err(pyo3::exceptions::PyValueError::new_err(
                    "expander requires the 'vabamorf' feature (not compiled)",
                ));
            }
        } else {
            rules
        };

        let strategy = ConflictStrategy::from_str(conflict_resolver)
            .map_err(|e| pyo3::exceptions::PyValueError::new_err(e))?;

        let config = TaggerConfig {
            output_layer: output_layer.to_string(),
            output_attributes: output_attributes.unwrap_or_default(),
            conflict_strategy: strategy,
            lowercase_text,
            group_attribute,
            priority_attribute,
            pattern_attribute,
            ambiguous_output_layer,
            unique_patterns,
            overlapped: false,
            match_attribute: None,
        };

        let tagger = SubstringTagger::new(rules, token_separators, config)
            .map_err(|e| pyo3::exceptions::PyValueError::new_err(e))?;

        Ok(Self { inner: tagger })
    }

    /// Tag text and return a layer dict.
    fn tag(&self, py: Python<'_>, text: &str) -> PyResult<PyObject> {
        let result = self.inner.tag(text);
        result.to_pydict(py)
    }

    /// Check if rules have inconsistent attribute sets.
    ///
    /// Returns True if some rules don't define the same set of attributes as others.
    /// Maps to EstNLTK's `AmbiguousRuleset.missing_attributes` property.
    #[getter]
    fn missing_attributes(&self) -> bool {
        self.inner.missing_attributes()
    }

    /// Return a dict mapping pattern strings to lists of rule dicts.
    ///
    /// Maps to EstNLTK's `Ruleset.rule_map` / `AmbiguousRuleset.rule_map` property.
    /// Each rule dict has keys: pattern, group, priority, attributes.
    #[getter]
    fn rule_map(&self, py: Python<'_>) -> PyResult<PyObject> {
        let map = self.inner.rule_map();
        let result = PyDict::new_bound(py);
        for (pattern, rule_indices) in &map {
            let rules_list = PyList::empty_bound(py);
            for &idx in rule_indices {
                let rule = &self.inner.rules[idx];
                let rule_dict = PyDict::new_bound(py);
                rule_dict.set_item("pattern", &rule.pattern_str)?;
                rule_dict.set_item("group", rule.group)?;
                rule_dict.set_item("priority", rule.priority)?;
                let attrs = PyDict::new_bound(py);
                for (k, v) in &rule.attributes {
                    attrs.set_item(k, v.to_pyobject(py))?;
                }
                rule_dict.set_item("attributes", attrs)?;
                rules_list.append(rule_dict)?;
            }
            result.set_item(pattern, rules_list)?;
        }
        Ok(result.unbind().into())
    }
}

/// Convenience function: tag text with substring patterns and return list of span dicts.
#[pyfunction]
#[pyo3(signature = (text, patterns, conflict_resolver="KEEP_MAXIMAL", lowercase_text=false, token_separators="", ambiguous_output_layer=true, expander=None, vabamorf=None))]
fn rs_substring_tag(
    py: Python<'_>,
    text: &str,
    patterns: &Bound<'_, PyList>,
    conflict_resolver: &str,
    lowercase_text: bool,
    token_separators: &str,
    ambiguous_output_layer: bool,
    expander: Option<&str>,
    vabamorf: Option<&Bound<'_, PyAny>>,
) -> PyResult<PyObject> {
    let mut rules = Vec::new();
    let mut all_attr_names = Vec::new();
    for item in patterns.iter() {
        let dict = item.downcast::<PyDict>().map_err(|_| {
            pyo3::exceptions::PyTypeError::new_err("Each pattern must be a dict")
        })?;
        let rule = parse_substring_pattern_dict(dict)?;
        for k in rule.attributes.keys() {
            if !all_attr_names.contains(k) {
                all_attr_names.push(k.clone());
            }
        }
        rules.push(rule);
    }

    // Apply expander if specified.
    let rules = if let Some(expander_name) = expander {
        #[cfg(feature = "vabamorf")]
        {
            let vm_obj = vabamorf.ok_or_else(|| {
                pyo3::exceptions::PyValueError::new_err(
                    "expander requires a vabamorf parameter (RsVabamorf instance)",
                )
            })?;
            let py_vm: &Bound<'_, PyVabamorf> = vm_obj.downcast().map_err(|_| {
                pyo3::exceptions::PyTypeError::new_err(
                    "vabamorf must be an RsVabamorf instance",
                )
            })?;
            let py_vm_ref = py_vm.borrow();
            let mut vm = py_vm_ref.inner.lock().unwrap();
            expander::expand_rules(rules, expander_name, &mut vm, lowercase_text)
                .map_err(|e| pyo3::exceptions::PyValueError::new_err(e))?
        }
        #[cfg(not(feature = "vabamorf"))]
        {
            let _ = vabamorf;
            let _ = expander_name;
            return Err(pyo3::exceptions::PyValueError::new_err(
                "expander requires the 'vabamorf' feature (not compiled)",
            ));
        }
    } else {
        rules
    };

    let strategy = ConflictStrategy::from_str(conflict_resolver)
        .map_err(|e| pyo3::exceptions::PyValueError::new_err(e))?;

    let config = TaggerConfig {
        output_layer: "substrings".to_string(),
        output_attributes: all_attr_names,
        conflict_strategy: strategy,
        lowercase_text,
        group_attribute: None,
        priority_attribute: None,
        pattern_attribute: None,
        ambiguous_output_layer,
        unique_patterns: false,
        overlapped: false,
        match_attribute: None,
    };

    let tagger = SubstringTagger::new(rules, token_separators, config)
        .map_err(|e| pyo3::exceptions::PyValueError::new_err(e))?;

    let result = tagger.tag(text);

    let list = PyList::empty_bound(py);
    for tagged in &result.spans {
        let d = PyDict::new_bound(py);
        d.set_item("base_span", (tagged.span.start, tagged.span.end))?;
        let anns = PyList::empty_bound(py);
        for ann in &tagged.annotations {
            anns.append(ann.to_pydict(py)?)?;
        }
        d.set_item("annotations", anns)?;
        list.append(d)?;
    }
    Ok(list.unbind().into())
}

/// Helper: resolve a Python column reference (str or int) to ColumnRef.
fn py_to_column_ref(obj: &Bound<'_, PyAny>) -> PyResult<ColumnRef> {
    if let Ok(i) = obj.extract::<usize>() {
        Ok(ColumnRef::Index(i))
    } else if let Ok(s) = obj.extract::<String>() {
        Ok(ColumnRef::Name(s))
    } else {
        Err(pyo3::exceptions::PyTypeError::new_err(
            "Column reference must be a str (column name) or int (column index)",
        ))
    }
}

/// Helper: convert CsvRules to Python list of dicts (same format as pattern dicts).
fn csv_rules_to_pylist(py: Python<'_>, rules: &[CsvRule]) -> PyResult<PyObject> {
    let list = PyList::empty_bound(py);
    for rule in rules {
        let dict = PyDict::new_bound(py);
        dict.set_item("pattern", &rule.pattern)?;
        dict.set_item("group", rule.group)?;
        dict.set_item("priority", rule.priority)?;
        let attrs = PyDict::new_bound(py);
        for (k, v) in &rule.attributes {
            attrs.set_item(k, v.to_pyobject(py))?;
        }
        dict.set_item("attributes", attrs)?;
        list.append(dict)?;
    }
    Ok(list.unbind().into())
}

/// Load extraction rules from a CSV file.
///
/// CSV format:
/// - Row 1: column names
/// - Row 2: column types (string, int, float, bool, regex)
/// - Row 3+: data rows
///
/// The `regex` type validates the cell value as a compilable regex pattern
/// (using resharp) at load time and stores it as a string. Invalid patterns
/// produce an error with the line number, column name, and parse error.
///
/// Returns a list of pattern dicts suitable for passing to RsRegexTagger or RsSubstringTagger.
#[pyfunction]
#[pyo3(signature = (file_path, key_column=None, group_column=None, priority_column=None))]
fn rs_load_rules_csv(
    py: Python<'_>,
    file_path: &str,
    key_column: Option<&Bound<'_, PyAny>>,
    group_column: Option<&Bound<'_, PyAny>>,
    priority_column: Option<&Bound<'_, PyAny>>,
) -> PyResult<PyObject> {
    let config = CsvLoadConfig {
        key_column: match key_column {
            Some(obj) => py_to_column_ref(obj)?,
            None => ColumnRef::Index(0),
        },
        group_column: match group_column {
            Some(obj) => Some(py_to_column_ref(obj)?),
            None => None,
        },
        priority_column: match priority_column {
            Some(obj) => Some(py_to_column_ref(obj)?),
            None => None,
        },
    };

    let rules = csv_loader::load_rules_from_csv(file_path, &config)
        .map_err(|e| pyo3::exceptions::PyValueError::new_err(e))?;

    csv_rules_to_pylist(py, &rules)
}

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
fn rs_string_list_pattern(
    strings: Vec<String>,
    replacements: Option<HashMap<String, String>>,
    ignore_case: bool,
    ignore_case_flags: Option<Vec<bool>>,
) -> PyResult<String> {
    let repl = replacements.unwrap_or_default();
    let flags_ref = ignore_case_flags.as_deref();
    string_list::build_string_list_pattern(&strings, &repl, ignore_case, flags_ref)
        .map_err(|e| pyo3::exceptions::PyValueError::new_err(e))
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
fn rs_choice_group_pattern(patterns: Vec<String>) -> PyResult<String> {
    string_list::build_choice_group_pattern(&patterns)
        .map_err(|e| pyo3::exceptions::PyValueError::new_err(e))
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
fn rs_merged_string_lists_pattern(
    string_lists: Vec<Vec<String>>,
    replacements: Option<HashMap<String, String>>,
    ignore_case: bool,
    ignore_case_flags_per_list: Option<Vec<Vec<bool>>>,
) -> PyResult<String> {
    let repl = replacements.unwrap_or_default();
    let flags_ref = ignore_case_flags_per_list.as_deref();
    string_list::build_merged_string_lists_pattern(&string_lists, &repl, ignore_case, flags_ref)
        .map_err(|e| pyo3::exceptions::PyValueError::new_err(e))
}

/// Build a regex pattern from a template with named placeholders.
///
/// Port of EstNLTK's `RegexPattern` from the `regex_library` subpackage.
/// The template uses `{name}` syntax for placeholders. Each placeholder is
/// replaced with the corresponding pattern from the `components` dict, wrapped
/// in a non-capture group `(?:...)` to prevent operator precedence issues.
///
/// Use `{{` and `}}` for literal braces in the template (e.g., `r"\d{{3}}"` → `\d{3}`).
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
fn rs_regex_pattern(template: &str, components: HashMap<String, String>) -> PyResult<String> {
    string_list::build_regex_pattern(template, &components)
        .map_err(|e| pyo3::exceptions::PyValueError::new_err(e))
}

// ── Vabamorf integration (feature-gated) ─────────────────────────────────────

#[cfg(feature = "vabamorf")]
#[pyclass(name = "RsVabamorf")]
struct PyVabamorf {
    inner: Mutex<vabamorf_rs::Vabamorf>,
}

#[cfg(feature = "vabamorf")]
#[pymethods]
impl PyVabamorf {
    #[new]
    fn new(dct_dir: &str) -> PyResult<Self> {
        let vm = vabamorf_rs::Vabamorf::from_dct_dir(std::path::Path::new(dct_dir))
            .map_err(|e| pyo3::exceptions::PyValueError::new_err(e.to_string()))?;
        Ok(Self {
            inner: Mutex::new(vm),
        })
    }

    /// Morphological analysis.
    #[pyo3(signature = (words, disambiguate=true, guess=true, phonetic=false, propername=true, stem=false))]
    fn analyze(
        &self,
        py: Python<'_>,
        words: Vec<String>,
        disambiguate: bool,
        guess: bool,
        phonetic: bool,
        propername: bool,
        stem: bool,
    ) -> PyResult<PyObject> {
        let word_refs: Vec<&str> = words.iter().map(|s| s.as_str()).collect();
        let mut vm = self.inner.lock().unwrap();
        let results = vm
            .analyze(&word_refs, disambiguate, guess, phonetic, propername, stem)
            .map_err(|e| pyo3::exceptions::PyRuntimeError::new_err(e.to_string()))?;

        let list = PyList::empty_bound(py);
        for word_analysis in &results {
            let d = PyDict::new_bound(py);
            d.set_item("word", &word_analysis.word)?;
            let analyses = PyList::empty_bound(py);
            for a in &word_analysis.analyses {
                let ad = PyDict::new_bound(py);
                ad.set_item("root", &a.root)?;
                ad.set_item("ending", &a.ending)?;
                ad.set_item("clitic", &a.clitic)?;
                ad.set_item("partofspeech", &a.partofspeech)?;
                ad.set_item("form", &a.form)?;
                analyses.append(ad)?;
            }
            d.set_item("analyses", analyses)?;
            list.append(d)?;
        }
        Ok(list.unbind().into())
    }

    /// Word synthesis.
    #[pyo3(signature = (lemma, form, partofspeech="", hint="", guess=true, phonetic=false))]
    fn synthesize(
        &self,
        lemma: &str,
        form: &str,
        partofspeech: &str,
        hint: &str,
        guess: bool,
        phonetic: bool,
    ) -> PyResult<Vec<String>> {
        let mut vm = self.inner.lock().unwrap();
        vm.synthesize(lemma, form, partofspeech, hint, guess, phonetic)
            .map_err(|e| pyo3::exceptions::PyRuntimeError::new_err(e.to_string()))
    }

    /// Spellcheck.
    #[pyo3(signature = (words, suggest=true))]
    fn spellcheck(&self, py: Python<'_>, words: Vec<String>, suggest: bool) -> PyResult<PyObject> {
        let word_refs: Vec<&str> = words.iter().map(|s| s.as_str()).collect();
        let mut vm = self.inner.lock().unwrap();
        let results = vm
            .spellcheck(&word_refs, suggest)
            .map_err(|e| pyo3::exceptions::PyRuntimeError::new_err(e.to_string()))?;

        let list = PyList::empty_bound(py);
        for sr in &results {
            let d = PyDict::new_bound(py);
            d.set_item("word", &sr.word)?;
            d.set_item("correct", sr.correct)?;
            let sug = PyList::empty_bound(py);
            for s in &sr.suggestions {
                sug.append(s)?;
            }
            d.set_item("suggestions", sug)?;
            list.append(d)?;
        }
        Ok(list.unbind().into())
    }

    /// Generate all 28 noun case forms.
    fn noun_forms_expander(&self, word: &str) -> PyResult<Vec<String>> {
        let mut vm = self.inner.lock().unwrap();
        expander::noun_forms_expander(&mut vm, word)
            .map_err(|e| pyo3::exceptions::PyRuntimeError::new_err(e))
    }

    /// Default expander (delegates to noun_forms_expander).
    fn default_expander(&self, word: &str) -> PyResult<Vec<String>> {
        let mut vm = self.inner.lock().unwrap();
        expander::default_expander(&mut vm, word)
            .map_err(|e| pyo3::exceptions::PyRuntimeError::new_err(e))
    }
}

/// Standalone: generate noun case forms using an RsVabamorf instance.
#[cfg(feature = "vabamorf")]
#[pyfunction]
fn rs_noun_forms_expander(vabamorf: &PyVabamorf, word: &str) -> PyResult<Vec<String>> {
    let mut vm = vabamorf.inner.lock().unwrap();
    expander::noun_forms_expander(&mut vm, word)
        .map_err(|e| pyo3::exceptions::PyRuntimeError::new_err(e))
}

/// Standalone: default expander using an RsVabamorf instance.
#[cfg(feature = "vabamorf")]
#[pyfunction]
fn rs_default_expander(vabamorf: &PyVabamorf, word: &str) -> PyResult<Vec<String>> {
    let mut vm = vabamorf.inner.lock().unwrap();
    expander::default_expander(&mut vm, word)
        .map_err(|e| pyo3::exceptions::PyRuntimeError::new_err(e))
}

/// Standalone: syllabify a word (does not require an RsVabamorf instance).
#[cfg(feature = "vabamorf")]
#[pyfunction]
fn rs_syllabify(py: Python<'_>, word: &str) -> PyResult<PyObject> {
    let syllables = vabamorf_rs::syllabify(word)
        .map_err(|e| pyo3::exceptions::PyRuntimeError::new_err(e.to_string()))?;
    let list = PyList::empty_bound(py);
    for s in &syllables {
        let d = PyDict::new_bound(py);
        d.set_item("syllable", &s.syllable)?;
        d.set_item("quantity", s.quantity)?;
        d.set_item("accent", s.accent)?;
        list.append(d)?;
    }
    Ok(list.unbind().into())
}

/// Python module definition.
#[pymodule]
fn estnltk_regex_rs(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_class::<PyRegexTagger>()?;
    m.add_class::<PySubstringTagger>()?;
    m.add_function(wrap_pyfunction!(rs_regex_tag, m)?)?;
    m.add_function(wrap_pyfunction!(rs_substring_tag, m)?)?;
    m.add_function(wrap_pyfunction!(rs_load_rules_csv, m)?)?;
    m.add_function(wrap_pyfunction!(rs_string_list_pattern, m)?)?;
    m.add_function(wrap_pyfunction!(rs_choice_group_pattern, m)?)?;
    m.add_function(wrap_pyfunction!(rs_merged_string_lists_pattern, m)?)?;
    m.add_function(wrap_pyfunction!(rs_regex_pattern, m)?)?;

    #[cfg(feature = "vabamorf")]
    {
        m.add_class::<PyVabamorf>()?;
        m.add_function(wrap_pyfunction!(rs_noun_forms_expander, m)?)?;
        m.add_function(wrap_pyfunction!(rs_default_expander, m)?)?;
        m.add_function(wrap_pyfunction!(rs_syllabify, m)?)?;
    }

    Ok(())
}
