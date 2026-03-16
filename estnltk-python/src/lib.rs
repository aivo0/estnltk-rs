mod py_helpers;
mod py_regex_tagger;
mod py_substring_tagger;
mod py_span_tagger;
mod py_phrase_tagger;
mod py_csv;
mod py_patterns;
#[cfg(feature = "vabamorf")]
mod py_vabamorf;

use pyo3::prelude::*;

/// Python module definition.
#[pymodule]
fn estnltk_regex_rs(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_class::<py_regex_tagger::PyRegexTagger>()?;
    m.add_class::<py_substring_tagger::PySubstringTagger>()?;
    m.add_class::<py_span_tagger::PySpanTagger>()?;
    m.add_class::<py_phrase_tagger::PyPhraseTagger>()?;
    m.add_function(wrap_pyfunction!(py_regex_tagger::rs_regex_tag, m)?)?;
    m.add_function(wrap_pyfunction!(py_substring_tagger::rs_substring_tag, m)?)?;
    m.add_function(wrap_pyfunction!(py_span_tagger::rs_span_tag, m)?)?;
    m.add_function(wrap_pyfunction!(py_phrase_tagger::rs_phrase_tag, m)?)?;
    m.add_function(wrap_pyfunction!(py_csv::rs_load_rules_csv, m)?)?;
    m.add_function(wrap_pyfunction!(py_patterns::rs_string_list_pattern, m)?)?;
    m.add_function(wrap_pyfunction!(py_patterns::rs_choice_group_pattern, m)?)?;
    m.add_function(wrap_pyfunction!(py_patterns::rs_merged_string_lists_pattern, m)?)?;
    m.add_function(wrap_pyfunction!(py_patterns::rs_regex_pattern, m)?)?;

    #[cfg(feature = "vabamorf")]
    {
        m.add_class::<py_vabamorf::PyVabamorf>()?;
        m.add_function(wrap_pyfunction!(py_vabamorf::rs_noun_forms_expander, m)?)?;
        m.add_function(wrap_pyfunction!(py_vabamorf::rs_default_expander, m)?)?;
        m.add_function(wrap_pyfunction!(py_vabamorf::rs_syllabify, m)?)?;
    }

    Ok(())
}
