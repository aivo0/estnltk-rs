use pyo3::prelude::*;
use pyo3::types::{PyDict, PyList};

use estnltk_core::{CommonConfig, ConflictStrategy, MatchSpan, TagResult};
use estnltk_taggers::{SpanRule, SpanTagger, SpanTaggerConfig};

use crate::py_helpers::{parse_pattern_fields, parse_tag_result};

/// Parse a Python pattern dict into a SpanRule.
pub fn parse_span_pattern_dict(dict: &Bound<'_, PyDict>) -> PyResult<SpanRule> {
    let (pattern, attributes, group, priority) = parse_pattern_fields(dict)?;
    Ok(SpanRule::new(&pattern, attributes, group, priority))
}

/// Tag an input layer dict directly from Python, avoiding full `TagResult`
/// materialization.
///
/// Only extracts the `input_attribute` value from each annotation (skipping
/// full dict conversion), then runs the standard sort -> resolve -> build
/// pipeline.
///
/// This is the logic that was previously in `SpanTagger::tag_from_py()`.
fn tag_from_py_dict(tagger: &SpanTagger, input: &Bound<'_, PyDict>) -> PyResult<TagResult> {
    let spans_obj = input
        .get_item("spans")?
        .ok_or_else(|| pyo3::exceptions::PyKeyError::new_err("'spans' key required"))?;
    let spans_list = spans_obj.downcast::<PyList>().map_err(|_| {
        pyo3::exceptions::PyTypeError::new_err("'spans' must be a list")
    })?;

    let mut all_matches: Vec<(MatchSpan, usize)> = Vec::new();

    for span_item in spans_list.iter() {
        let span_dict = span_item.downcast::<PyDict>().map_err(|_| {
            pyo3::exceptions::PyTypeError::new_err("Each span must be a dict")
        })?;

        let base_span: (usize, usize) = span_dict
            .get_item("base_span")?
            .ok_or_else(|| {
                pyo3::exceptions::PyKeyError::new_err("'base_span' key required")
            })?
            .extract()?;
        let match_span = MatchSpan::new(base_span.0, base_span.1);

        let ann_obj = span_dict
            .get_item("annotations")?
            .ok_or_else(|| {
                pyo3::exceptions::PyKeyError::new_err("'annotations' key required")
            })?;
        let ann_list = ann_obj.downcast::<PyList>().map_err(|_| {
            pyo3::exceptions::PyTypeError::new_err("'annotations' must be a list")
        })?;

        for ann_item in ann_list.iter() {
            let ann_dict = ann_item.downcast::<PyDict>().map_err(|_| {
                pyo3::exceptions::PyTypeError::new_err("Each annotation must be a dict")
            })?;

            if let Some(val_obj) = ann_dict.get_item(&tagger.config.input_attribute)? {
                if val_obj.is_none() {
                    continue;
                }
                // Extract string representation matching the Rust-side logic.
                // Check bool before int because Python bool is a subclass of int.
                let value_str: String = if let Ok(s) = val_obj.extract::<String>() {
                    s
                } else if let Ok(b) = val_obj.extract::<bool>() {
                    b.to_string()
                } else if let Ok(i) = val_obj.extract::<i64>() {
                    i.to_string()
                } else if let Ok(f) = val_obj.extract::<f64>() {
                    f.to_string()
                } else {
                    continue;
                };

                tagger.lookup_rules(&value_str, match_span, &mut all_matches);
            }
        }
    }

    all_matches.sort_by_key(|&(span, _)| (span.start, span.end));

    let resolved = estnltk_core::resolve_conflicts(
        tagger.config.common.conflict_strategy,
        &all_matches,
        |rule_idx| (tagger.rules[rule_idx].group as i32, tagger.rules[rule_idx].priority),
    );

    Ok(tagger.build_result(&resolved))
}

/// Python-exposed SpanTagger class.
///
/// Matches attribute values from an input layer against a ruleset of exact
/// string patterns.  Takes a layer dict (output of `RsRegexTagger.tag()` or
/// `RsSubstringTagger.tag()`) as input.
#[pyclass(name = "RsSpanTagger")]
pub struct PySpanTagger {
    pub inner: SpanTagger,
}

#[pymethods]
impl PySpanTagger {
    #[new]
    #[pyo3(signature = (patterns, input_attribute, output_layer="spans", output_attributes=None, conflict_resolver="KEEP_MAXIMAL", ignore_case=false, group_attribute=None, priority_attribute=None, pattern_attribute=None, ambiguous_output_layer=true, unique_patterns=false))]
    fn new(
        patterns: &Bound<'_, PyList>,
        input_attribute: &str,
        output_layer: &str,
        output_attributes: Option<Vec<String>>,
        conflict_resolver: &str,
        ignore_case: bool,
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
            rules.push(parse_span_pattern_dict(dict)?);
        }

        let strategy = conflict_resolver.parse::<ConflictStrategy>()
            ?;

        let config = SpanTaggerConfig {
            common: CommonConfig {
                output_layer: output_layer.to_string(),
                output_attributes: output_attributes.unwrap_or_default(),
                conflict_strategy: strategy,
                group_attribute,
                priority_attribute,
                pattern_attribute,
                ambiguous_output_layer,
                unique_patterns,
            },
            input_attribute: input_attribute.to_string(),
            ignore_case,
        };

        let tagger = SpanTagger::new(rules, config)
            ?;

        Ok(Self { inner: tagger })
    }

    /// Tag an input layer dict and return a new layer dict.
    ///
    /// The input must be a dict with the same format as returned by
    /// `RsRegexTagger.tag()` or `RsSubstringTagger.tag()`:
    /// `{"name": ..., "attributes": [...], "ambiguous": ..., "spans": [...]}`
    ///
    /// Uses `tag_from_py_dict` to iterate Python spans directly without
    /// materializing a full `TagResult`, extracting only the `input_attribute`
    /// value from each annotation.
    fn tag(&self, py: Python<'_>, input_layer: &Bound<'_, PyDict>) -> PyResult<PyObject> {
        let result = tag_from_py_dict(&self.inner, input_layer)?;
        result.to_pydict(py)
    }

    /// Check if rules have inconsistent attribute sets.
    #[getter]
    fn missing_attributes(&self) -> bool {
        self.inner.missing_attributes()
    }

    /// Return a dict mapping pattern strings to lists of rule dicts.
    #[getter]
    fn rule_map(&self, py: Python<'_>) -> PyResult<PyObject> {
        let result = PyDict::new_bound(py);
        for (pattern, rule_indices) in self.inner.rule_map() {
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

/// Convenience function: tag an input layer with span patterns.
#[pyfunction]
#[pyo3(signature = (input_layer, patterns, input_attribute, conflict_resolver="KEEP_MAXIMAL", ignore_case=false, ambiguous_output_layer=true))]
pub fn rs_span_tag(
    py: Python<'_>,
    input_layer: &Bound<'_, PyDict>,
    patterns: &Bound<'_, PyList>,
    input_attribute: &str,
    conflict_resolver: &str,
    ignore_case: bool,
    ambiguous_output_layer: bool,
) -> PyResult<PyObject> {
    let mut rules = Vec::new();
    let mut all_attr_names = Vec::new();
    for item in patterns.iter() {
        let dict = item.downcast::<PyDict>().map_err(|_| {
            pyo3::exceptions::PyTypeError::new_err("Each pattern must be a dict")
        })?;
        let rule = parse_span_pattern_dict(dict)?;
        for k in rule.attributes.keys() {
            if !all_attr_names.contains(k) {
                all_attr_names.push(k.clone());
            }
        }
        rules.push(rule);
    }

    let strategy = conflict_resolver.parse::<ConflictStrategy>()
        ?;

    let config = SpanTaggerConfig {
        common: CommonConfig {
            output_layer: "spans".to_string(),
            output_attributes: all_attr_names,
            conflict_strategy: strategy,
            group_attribute: None,
            priority_attribute: None,
            pattern_attribute: None,
            ambiguous_output_layer,
            unique_patterns: false,
        },
        input_attribute: input_attribute.to_string(),
        ignore_case,
    };

    let tagger = SpanTagger::new(rules, config)
        ?;

    let input = parse_tag_result(input_layer)?;
    let result = tagger.tag(&input);

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
