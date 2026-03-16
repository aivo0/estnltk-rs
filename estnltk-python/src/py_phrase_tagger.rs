use std::collections::HashMap;

use pyo3::prelude::*;
use pyo3::types::{PyDict, PyList};

use estnltk_core::{AnnotationValue, ConflictStrategy};
use estnltk_taggers::{make_phrase_rule, PhraseTagger, PhraseTaggerConfig, PhraseRule};

use crate::py_helpers::parse_tag_result;

/// Parse a Python pattern dict into a PhraseRule.
///
/// The pattern is a list/tuple of strings (the phrase), e.g., `["euroopa", "liit"]`.
pub fn parse_phrase_pattern_dict(
    dict: &Bound<'_, PyDict>,
) -> PyResult<PhraseRule> {
    let pattern_obj = dict
        .get_item("pattern")?
        .ok_or_else(|| pyo3::exceptions::PyKeyError::new_err("'pattern' key required"))?;

    // Extract pattern as Vec<String> from list or tuple.
    let pattern: Vec<String> = pattern_obj.extract().map_err(|_| {
        pyo3::exceptions::PyTypeError::new_err(
            "'pattern' must be a list or tuple of strings for PhraseTagger",
        )
    })?;

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

    Ok(make_phrase_rule(pattern, attributes, group, priority))
}

/// Python-exposed PhraseTagger class.
///
/// Matches sequential attribute values (phrase tuples) from an input layer
/// against a ruleset of phrase patterns.  Produces an enveloping layer where
/// each output span wraps multiple input spans.
///
/// Takes a layer dict (output of `RsRegexTagger.tag()`, `RsSubstringTagger.tag()`,
/// `RsSpanTagger.tag()`, or another tagger) as input.
#[pyclass(name = "RsPhraseTagger")]
pub struct PyPhraseTagger {
    pub inner: PhraseTagger,
}

#[pymethods]
impl PyPhraseTagger {
    #[new]
    #[pyo3(signature = (patterns, input_attribute, output_layer="phrases", output_attributes=None, conflict_resolver="KEEP_MAXIMAL", ignore_case=false, phrase_attribute="phrase", group_attribute=None, priority_attribute=None, pattern_attribute=None, ambiguous_output_layer=true, unique_patterns=false))]
    fn new(
        patterns: &Bound<'_, PyList>,
        input_attribute: &str,
        output_layer: &str,
        output_attributes: Option<Vec<String>>,
        conflict_resolver: &str,
        ignore_case: bool,
        phrase_attribute: Option<&str>,
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
            rules.push(parse_phrase_pattern_dict(dict)?);
        }

        let strategy = conflict_resolver.parse::<ConflictStrategy>()
            ?;

        let config = PhraseTaggerConfig {
            output_layer: output_layer.to_string(),
            input_attribute: input_attribute.to_string(),
            output_attributes: output_attributes.unwrap_or_default(),
            conflict_strategy: strategy,
            ignore_case,
            phrase_attribute: phrase_attribute.map(|s| s.to_string()),
            group_attribute,
            priority_attribute,
            pattern_attribute,
            ambiguous_output_layer,
            unique_patterns,
        };

        let tagger = PhraseTagger::new(rules, config)
            ?;

        Ok(Self { inner: tagger })
    }

    /// Tag an input layer dict and return a new enveloping layer dict.
    ///
    /// The input must be a dict with the same format as returned by
    /// `RsRegexTagger.tag()`, `RsSubstringTagger.tag()`, or `RsSpanTagger.tag()`:
    /// `{"name": ..., "attributes": [...], "ambiguous": ..., "spans": [...]}`
    fn tag(&self, py: Python<'_>, input_layer: &Bound<'_, PyDict>) -> PyResult<PyObject> {
        let input = parse_tag_result(input_layer)?;
        let result = self.inner.tag(&input);
        result.to_pydict(py)
    }

    /// Check if rules have inconsistent attribute sets.
    #[getter]
    fn missing_attributes(&self) -> bool {
        self.inner.missing_attributes()
    }

    /// Return a dict mapping phrase pattern tuples to lists of rule dicts.
    #[getter]
    fn rule_map(&self, py: Python<'_>) -> PyResult<PyObject> {
        let result = PyDict::new_bound(py);
        for (pattern, rule_indices) in self.inner.rule_map() {
            // Convert pattern Vec<String> to Python tuple.
            let py_pattern = pyo3::types::PyTuple::new_bound(
                py,
                pattern.iter().map(|s| s.to_object(py)),
            );
            let rules_list = PyList::empty_bound(py);
            for &idx in rule_indices {
                let rule = &self.inner.rules[idx];
                let rule_dict = PyDict::new_bound(py);
                let py_rule_pattern = pyo3::types::PyTuple::new_bound(
                    py,
                    rule.pattern.iter().map(|s| s.to_object(py)),
                );
                rule_dict.set_item("pattern", py_rule_pattern)?;
                rule_dict.set_item("group", rule.group)?;
                rule_dict.set_item("priority", rule.priority)?;
                let attrs = PyDict::new_bound(py);
                for (k, v) in &rule.attributes {
                    attrs.set_item(k, v.to_pyobject(py))?;
                }
                rule_dict.set_item("attributes", attrs)?;
                rules_list.append(rule_dict)?;
            }
            result.set_item(py_pattern, rules_list)?;
        }
        Ok(result.unbind().into())
    }
}

/// Convenience function: tag an input layer with phrase patterns.
#[pyfunction]
#[pyo3(signature = (input_layer, patterns, input_attribute, conflict_resolver="KEEP_MAXIMAL", ignore_case=false, ambiguous_output_layer=true, phrase_attribute="phrase"))]
pub fn rs_phrase_tag(
    py: Python<'_>,
    input_layer: &Bound<'_, PyDict>,
    patterns: &Bound<'_, PyList>,
    input_attribute: &str,
    conflict_resolver: &str,
    ignore_case: bool,
    ambiguous_output_layer: bool,
    phrase_attribute: Option<&str>,
) -> PyResult<PyObject> {
    let mut rules = Vec::new();
    let mut all_attr_names = Vec::new();
    for item in patterns.iter() {
        let dict = item.downcast::<PyDict>().map_err(|_| {
            pyo3::exceptions::PyTypeError::new_err("Each pattern must be a dict")
        })?;
        let rule = parse_phrase_pattern_dict(dict)?;
        for k in rule.attributes.keys() {
            if !all_attr_names.contains(k) {
                all_attr_names.push(k.clone());
            }
        }
        rules.push(rule);
    }

    let strategy = conflict_resolver.parse::<ConflictStrategy>()
        ?;

    let config = PhraseTaggerConfig {
        output_layer: "phrases".to_string(),
        input_attribute: input_attribute.to_string(),
        output_attributes: all_attr_names,
        conflict_strategy: strategy,
        ignore_case,
        phrase_attribute: phrase_attribute.map(|s| s.to_string()),
        group_attribute: None,
        priority_attribute: None,
        pattern_attribute: None,
        ambiguous_output_layer,
        unique_patterns: false,
    };

    let tagger = PhraseTagger::new(rules, config)
        ?;

    let input = parse_tag_result(input_layer)?;
    let result = tagger.tag(&input);

    let list = PyList::empty_bound(py);
    for tagged in &result.spans {
        let d = PyDict::new_bound(py);

        // base_span: tuple of (start, end) tuples
        let base_span_parts: Vec<pyo3::Py<pyo3::types::PyTuple>> = tagged
            .spans
            .iter()
            .map(|s| pyo3::types::PyTuple::new_bound(py, &[s.start, s.end]).unbind())
            .collect();
        let py_base_span = pyo3::types::PyTuple::new_bound(py, &base_span_parts);
        d.set_item("base_span", py_base_span)?;

        let anns = PyList::empty_bound(py);
        for ann in &tagged.annotations {
            anns.append(ann.to_pydict(py)?)?;
        }
        d.set_item("annotations", anns)?;
        list.append(d)?;
    }
    Ok(list.unbind().into())
}
