pub mod byte_char;
pub mod conflict;
pub mod csv_loader;
pub mod string_list;
pub mod substring_tagger;
pub mod tagger;
pub mod types;

use std::collections::HashMap;

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
    #[pyo3(signature = (patterns, output_layer="substrings", output_attributes=None, conflict_resolver="KEEP_MAXIMAL", lowercase_text=false, token_separators="", group_attribute=None, priority_attribute=None, pattern_attribute=None, ambiguous_output_layer=true, unique_patterns=false))]
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
    ) -> PyResult<Self> {
        let mut rules = Vec::new();
        for item in patterns.iter() {
            let dict = item.downcast::<PyDict>().map_err(|_| {
                pyo3::exceptions::PyTypeError::new_err("Each pattern must be a dict")
            })?;
            rules.push(parse_substring_pattern_dict(dict)?);
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
}

/// Convenience function: tag text with substring patterns and return list of span dicts.
#[pyfunction]
#[pyo3(signature = (text, patterns, conflict_resolver="KEEP_MAXIMAL", lowercase_text=false, token_separators="", ambiguous_output_layer=true))]
fn rs_substring_tag(
    py: Python<'_>,
    text: &str,
    patterns: &Bound<'_, PyList>,
    conflict_resolver: &str,
    lowercase_text: bool,
    token_separators: &str,
    ambiguous_output_layer: bool,
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
/// - Row 2: column types (string, int, float, bool)
/// - Row 3+: data rows
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

/// Python module definition.
#[pymodule]
fn estnltk_regex_rs(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_class::<PyRegexTagger>()?;
    m.add_class::<PySubstringTagger>()?;
    m.add_function(wrap_pyfunction!(rs_regex_tag, m)?)?;
    m.add_function(wrap_pyfunction!(rs_substring_tag, m)?)?;
    m.add_function(wrap_pyfunction!(rs_load_rules_csv, m)?)?;
    m.add_function(wrap_pyfunction!(rs_string_list_pattern, m)?)?;
    Ok(())
}
