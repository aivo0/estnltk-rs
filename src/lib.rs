pub mod byte_char;
pub mod conflict;
pub mod substring_tagger;
pub mod tagger;
pub mod types;

use std::collections::HashMap;

use pyo3::prelude::*;
use pyo3::types::{PyDict, PyList};

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
    #[pyo3(signature = (patterns, output_layer="regexes", output_attributes=None, conflict_resolver="KEEP_MAXIMAL", lowercase_text=false, group_attribute=None, priority_attribute=None, pattern_attribute=None))]
    fn new(
        patterns: &Bound<'_, PyList>,
        output_layer: &str,
        output_attributes: Option<Vec<String>>,
        conflict_resolver: &str,
        lowercase_text: bool,
        group_attribute: Option<String>,
        priority_attribute: Option<String>,
        pattern_attribute: Option<String>,
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
#[pyo3(signature = (text, patterns, conflict_resolver="KEEP_MAXIMAL", lowercase_text=false))]
fn rs_regex_tag(
    py: Python<'_>,
    text: &str,
    patterns: &Bound<'_, PyList>,
    conflict_resolver: &str,
    lowercase_text: bool,
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
    #[pyo3(signature = (patterns, output_layer="substrings", output_attributes=None, conflict_resolver="KEEP_MAXIMAL", lowercase_text=false, token_separators="", group_attribute=None, priority_attribute=None, pattern_attribute=None))]
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
}

/// Convenience function: tag text with substring patterns and return list of span dicts.
#[pyfunction]
#[pyo3(signature = (text, patterns, conflict_resolver="KEEP_MAXIMAL", lowercase_text=false, token_separators=""))]
fn rs_substring_tag(
    py: Python<'_>,
    text: &str,
    patterns: &Bound<'_, PyList>,
    conflict_resolver: &str,
    lowercase_text: bool,
    token_separators: &str,
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

/// Python module definition.
#[pymodule]
fn estnltk_regex_rs(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_class::<PyRegexTagger>()?;
    m.add_class::<PySubstringTagger>()?;
    m.add_function(wrap_pyfunction!(rs_regex_tag, m)?)?;
    m.add_function(wrap_pyfunction!(rs_substring_tag, m)?)?;
    Ok(())
}
