use pyo3::prelude::*;
use pyo3::types::{PyDict, PyList};

use estnltk_core::{ConflictStrategy, TaggerConfig};
use estnltk_taggers::{SubstringRule, SubstringTagger};

use crate::py_helpers::parse_pattern_fields;
#[cfg(feature = "vabamorf")]
use crate::py_vabamorf::PyVabamorf;

/// Parse a Python pattern dict into a SubstringRule.
pub fn parse_substring_pattern_dict(dict: &Bound<'_, PyDict>) -> PyResult<SubstringRule> {
    let (pattern, attributes, group, priority) = parse_pattern_fields(dict)?;
    Ok(SubstringRule::new(&pattern, attributes, group, priority))
}

/// Python-exposed SubstringTagger class.
#[pyclass(name = "RsSubstringTagger")]
pub struct PySubstringTagger {
    pub inner: SubstringTagger,
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
                estnltk_morph::expand_rules(rules, expander_name, &mut vm, lowercase_text)
                    ?
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
            overlapped: false,
            match_attribute: None,
        };

        let tagger = SubstringTagger::new(rules, token_separators, config)
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
}

/// Convenience function: tag text with substring patterns and return list of span dicts.
#[pyfunction]
#[pyo3(signature = (text, patterns, conflict_resolver="KEEP_MAXIMAL", lowercase_text=false, token_separators="", ambiguous_output_layer=true, expander=None, vabamorf=None))]
pub fn rs_substring_tag(
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
            estnltk_morph::expand_rules(rules, expander_name, &mut vm, lowercase_text)
                ?
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

    let strategy = conflict_resolver.parse::<ConflictStrategy>()
        ?;

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
