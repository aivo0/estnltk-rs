use pyo3::prelude::*;

use estnltk_csv::{load_rules_from_csv, CsvLoadConfig, ColumnRef};

use crate::py_helpers::{csv_rules_to_pylist, py_to_column_ref};

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
pub fn rs_load_rules_csv(
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

    let rules = load_rules_from_csv(file_path, &config)
        ?;

    csv_rules_to_pylist(py, &rules)
}
