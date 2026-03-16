use std::collections::HashMap;

use pyo3::prelude::*;
use pyo3::types::{PyDict, PyList};

use estnltk_core::{
    Annotation, AnnotationValue, MatchSpan, TagResult, TaggedSpan,
};
use estnltk_csv::{ColumnRef, CsvRule};

/// Extract common fields (pattern, attributes, group, priority) from a Python
/// pattern dict.  Shared by all `parse_*_pattern_dict` functions.
pub fn parse_pattern_fields(
    dict: &Bound<'_, PyDict>,
) -> PyResult<(String, HashMap<String, AnnotationValue>, u32, i32)> {
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

    Ok((pattern, attributes, group, priority))
}

/// Parse a Python layer dict (from `tag()` output) into a `TagResult`.
pub fn parse_tag_result(dict: &Bound<'_, PyDict>) -> PyResult<TagResult> {
    let name: String = dict
        .get_item("name")?
        .ok_or_else(|| pyo3::exceptions::PyKeyError::new_err("'name' key required"))?
        .extract()?;

    let attributes: Vec<String> = dict
        .get_item("attributes")?
        .ok_or_else(|| pyo3::exceptions::PyKeyError::new_err("'attributes' key required"))?
        .extract()?;

    let ambiguous: bool = dict
        .get_item("ambiguous")?
        .map(|v| v.extract())
        .unwrap_or(Ok(true))?;

    let spans_obj = dict
        .get_item("spans")?
        .ok_or_else(|| pyo3::exceptions::PyKeyError::new_err("'spans' key required"))?;
    let spans_list = spans_obj.downcast::<PyList>().map_err(|_| {
        pyo3::exceptions::PyTypeError::new_err("'spans' must be a list")
    })?;

    let mut spans = Vec::new();
    for span_item in spans_list.iter() {
        let span_dict = span_item.downcast::<PyDict>().map_err(|_| {
            pyo3::exceptions::PyTypeError::new_err("Each span must be a dict")
        })?;

        let base_span: (usize, usize) = span_dict
            .get_item("base_span")?
            .ok_or_else(|| pyo3::exceptions::PyKeyError::new_err("'base_span' key required"))?
            .extract()?;

        let ann_obj = span_dict
            .get_item("annotations")?
            .ok_or_else(|| pyo3::exceptions::PyKeyError::new_err("'annotations' key required"))?;
        let ann_list = ann_obj.downcast::<PyList>().map_err(|_| {
            pyo3::exceptions::PyTypeError::new_err("'annotations' must be a list")
        })?;

        let mut annotations = Vec::new();
        for ann_item in ann_list.iter() {
            let ann_dict = ann_item.downcast::<PyDict>().map_err(|_| {
                pyo3::exceptions::PyTypeError::new_err("Each annotation must be a dict")
            })?;
            let mut annotation = Annotation::new();
            for (k, v) in ann_dict.iter() {
                let key: String = k.extract()?;
                let val = AnnotationValue::from_pyobject(&v)?;
                annotation.insert(key, val);
            }
            annotations.push(annotation);
        }

        spans.push(TaggedSpan {
            span: MatchSpan::new(base_span.0, base_span.1),
            annotations,
        });
    }

    Ok(TagResult {
        name,
        attributes,
        ambiguous,
        spans,
    })
}

/// Helper: convert CsvRules to Python list of dicts (same format as pattern dicts).
pub fn csv_rules_to_pylist(py: Python<'_>, rules: &[CsvRule]) -> PyResult<PyObject> {
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

/// Helper: resolve a Python column reference (str or int) to ColumnRef.
pub fn py_to_column_ref(obj: &Bound<'_, PyAny>) -> PyResult<ColumnRef> {
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
