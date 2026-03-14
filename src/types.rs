use pyo3::prelude::*;
use pyo3::types::{PyDict, PyList};
use std::collections::{HashMap, HashSet};

/// Character-level span (not byte-level).
/// Maps to EstNLTK's `ElementaryBaseSpan`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct MatchSpan {
    pub start: usize,
    pub end: usize,
}

impl MatchSpan {
    pub fn new(start: usize, end: usize) -> Self {
        Self { start, end }
    }

    /// True if `self` and `other` overlap (share at least one position).
    pub fn overlaps(&self, other: &MatchSpan) -> bool {
        self.start < other.end && other.start < self.end
    }
}

/// Dynamic annotation value — mirrors Python's duck-typed annotation values.
#[derive(Debug, Clone, PartialEq)]
pub enum AnnotationValue {
    Str(String),
    Int(i64),
    Float(f64),
    Bool(bool),
    Null,
}

impl AnnotationValue {
    pub fn to_pyobject(&self, py: Python<'_>) -> PyObject {
        match self {
            AnnotationValue::Str(s) => s.to_object(py),
            AnnotationValue::Int(i) => i.to_object(py),
            AnnotationValue::Float(f) => f.to_object(py),
            AnnotationValue::Bool(b) => b.to_object(py),
            AnnotationValue::Null => py.None(),
        }
    }

    pub fn from_pyobject(obj: &Bound<'_, PyAny>) -> PyResult<Self> {
        if obj.is_none() {
            return Ok(AnnotationValue::Null);
        }
        if let Ok(b) = obj.extract::<bool>() {
            return Ok(AnnotationValue::Bool(b));
        }
        if let Ok(i) = obj.extract::<i64>() {
            return Ok(AnnotationValue::Int(i));
        }
        if let Ok(f) = obj.extract::<f64>() {
            return Ok(AnnotationValue::Float(f));
        }
        if let Ok(s) = obj.extract::<String>() {
            return Ok(AnnotationValue::Str(s));
        }
        Err(pyo3::exceptions::PyTypeError::new_err(
            "Unsupported annotation value type; expected str, int, float, bool, or None",
        ))
    }
}

/// A single annotation: attribute name → value.
/// Maps to EstNLTK's `Annotation` (a dict).
#[derive(Debug, Clone, PartialEq)]
pub struct Annotation(pub HashMap<String, AnnotationValue>);

impl Annotation {
    pub fn new() -> Self {
        Self(HashMap::new())
    }

    pub fn to_pydict(&self, py: Python<'_>) -> PyResult<Py<PyDict>> {
        let dict = PyDict::new_bound(py);
        for (k, v) in &self.0 {
            dict.set_item(k, v.to_pyobject(py))?;
        }
        Ok(dict.unbind())
    }
}

/// A span with its annotations.
/// Maps to EstNLTK's `Span` (base_span + annotations list).
#[derive(Debug, Clone)]
pub struct TaggedSpan {
    pub span: MatchSpan,
    pub annotations: Vec<Annotation>,
}

/// The result of tagging — maps to an EstNLTK `Layer` dict.
#[derive(Debug, Clone)]
pub struct TagResult {
    pub name: String,
    pub attributes: Vec<String>,
    pub ambiguous: bool,
    pub spans: Vec<TaggedSpan>,
}

impl TagResult {
    /// Convert to Python dict matching EstNLTK's `layer_to_dict()` format.
    pub fn to_pydict(&self, py: Python<'_>) -> PyResult<PyObject> {
        let dict = PyDict::new_bound(py);
        dict.set_item("name", &self.name)?;
        let attr_strs: Vec<&str> = self.attributes.iter().map(|s| s.as_str()).collect();
        let attrs = PyList::new_bound(py, &attr_strs);
        dict.set_item("attributes", attrs)?;
        dict.set_item("ambiguous", self.ambiguous)?;

        let spans_list = PyList::empty_bound(py);
        for tagged in &self.spans {
            let span_dict = PyDict::new_bound(py);
            let base_span = (tagged.span.start, tagged.span.end);
            span_dict.set_item("base_span", base_span)?;

            let ann_list = PyList::empty_bound(py);
            for ann in &tagged.annotations {
                ann_list.append(ann.to_pydict(py)?)?;
            }
            span_dict.set_item("annotations", ann_list)?;
            spans_list.append(span_dict)?;
        }
        dict.set_item("spans", spans_list)?;

        Ok(dict.unbind().into())
    }
}

/// A compiled extraction rule.
/// Maps to EstNLTK's `StaticExtractionRule`.
pub struct ExtractionRule {
    pub pattern_str: String,
    pub compiled: resharp::Regex,
    pub attributes: HashMap<String, AnnotationValue>,
    pub group: u32,
    pub priority: i32,
}

impl std::fmt::Debug for ExtractionRule {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ExtractionRule")
            .field("pattern_str", &self.pattern_str)
            .field("attributes", &self.attributes)
            .field("group", &self.group)
            .field("priority", &self.priority)
            .finish()
    }
}

/// Conflict resolution strategy.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConflictStrategy {
    KeepAll,
    KeepMaximal,
    KeepMinimal,
    KeepAllExceptPriority,
    KeepMaximalExceptPriority,
    KeepMinimalExceptPriority,
}

impl ConflictStrategy {
    pub fn from_str(s: &str) -> Result<Self, String> {
        match s {
            "KEEP_ALL" => Ok(ConflictStrategy::KeepAll),
            "KEEP_MAXIMAL" => Ok(ConflictStrategy::KeepMaximal),
            "KEEP_MINIMAL" => Ok(ConflictStrategy::KeepMinimal),
            "KEEP_ALL_EXCEPT_PRIORITY" => Ok(ConflictStrategy::KeepAllExceptPriority),
            "KEEP_MAXIMAL_EXCEPT_PRIORITY" => Ok(ConflictStrategy::KeepMaximalExceptPriority),
            "KEEP_MINIMAL_EXCEPT_PRIORITY" => Ok(ConflictStrategy::KeepMinimalExceptPriority),
            other => Err(format!("Unknown conflict resolver: '{}'", other)),
        }
    }
}

/// Tagger configuration.
#[derive(Debug)]
pub struct TaggerConfig {
    pub output_layer: String,
    pub output_attributes: Vec<String>,
    pub conflict_strategy: ConflictStrategy,
    pub lowercase_text: bool,
    pub group_attribute: Option<String>,
    pub priority_attribute: Option<String>,
    pub pattern_attribute: Option<String>,
}

/// Check if rules have inconsistent attribute sets.
///
/// Returns `true` if some rules don't define the same set of attributes as others.
/// Maps to EstNLTK's `AmbiguousRuleset.missing_attributes` property.
pub fn has_missing_attributes(rules_attrs: &[&HashMap<String, AnnotationValue>]) -> bool {
    if rules_attrs.len() <= 1 {
        return false;
    }
    let first_keys: HashSet<&String> = rules_attrs[0].keys().collect();
    for attrs in &rules_attrs[1..] {
        let keys: HashSet<&String> = attrs.keys().collect();
        if keys != first_keys {
            return true;
        }
    }
    false
}

/// Normalize an annotation so it contains all `output_attributes` keys.
///
/// Missing attributes are filled with `AnnotationValue::Null`, matching
/// EstNLTK's `Layer.add_annotation()` behavior where missing attributes
/// get `None` (the layer's default value).
pub fn normalize_annotation(annotation: &mut Annotation, output_attributes: &[String]) {
    for attr_name in output_attributes {
        if !annotation.0.contains_key(attr_name) {
            annotation.0.insert(attr_name.clone(), AnnotationValue::Null);
        }
    }
}
