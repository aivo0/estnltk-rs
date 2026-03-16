use pyo3::prelude::*;
use pyo3::types::{PyDict, PyList};

use estnltk_core::{byte_to_char_map, ConflictStrategy, TaggerConfig};
use estnltk_taggers::{make_rule, ExtractionRule, RegexTagger};

use crate::py_helpers::parse_pattern_fields;

/// Parse a Python pattern dict into an ExtractionRule.
pub fn parse_pattern_dict(dict: &Bound<'_, PyDict>) -> PyResult<ExtractionRule> {
    let (pattern, attributes, group, priority) = parse_pattern_fields(dict)?;
    make_rule(&pattern, attributes, group, priority)
        .map_err(PyErr::from)
}

/// Python-exposed RegexTagger class.
#[pyclass(name = "RsRegexTagger")]
pub struct PyRegexTagger {
    pub inner: RegexTagger,
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

        let strategy = conflict_resolver.parse::<ConflictStrategy>()
            ?;

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
            ?;

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
        let raw_text: std::borrow::Cow<str> = if self.inner.config.lowercase_text {
            std::borrow::Cow::Owned(text.to_lowercase())
        } else {
            std::borrow::Cow::Borrowed(text)
        };
        let b2c = byte_to_char_map(&raw_text);
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
pub fn rs_regex_tag(
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

    let strategy = conflict_resolver.parse::<ConflictStrategy>()
        ?;

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
        ?;

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
