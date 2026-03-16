use std::sync::Mutex;

use pyo3::prelude::*;
use pyo3::types::{PyDict, PyList};

/// Python-exposed Vabamorf morphological analyzer.
#[pyclass(name = "RsVabamorf")]
pub struct PyVabamorf {
    pub inner: Mutex<vabamorf_rs::Vabamorf>,
}

#[pymethods]
impl PyVabamorf {
    #[new]
    fn new(dct_dir: &str) -> PyResult<Self> {
        let vm = vabamorf_rs::Vabamorf::from_dct_dir(std::path::Path::new(dct_dir))
            .map_err(|e| pyo3::exceptions::PyValueError::new_err(e.to_string()))?;
        Ok(Self {
            inner: Mutex::new(vm),
        })
    }

    /// Morphological analysis.
    #[pyo3(signature = (words, disambiguate=true, guess=true, phonetic=false, propername=true, stem=false))]
    fn analyze(
        &self,
        py: Python<'_>,
        words: Vec<String>,
        disambiguate: bool,
        guess: bool,
        phonetic: bool,
        propername: bool,
        stem: bool,
    ) -> PyResult<PyObject> {
        let word_refs: Vec<&str> = words.iter().map(|s| s.as_str()).collect();
        let mut vm = self.inner.lock().unwrap();
        let results = vm
            .analyze(&word_refs, disambiguate, guess, phonetic, propername, stem)
            .map_err(|e| pyo3::exceptions::PyRuntimeError::new_err(e.to_string()))?;

        let list = PyList::empty_bound(py);
        for word_analysis in &results {
            let d = PyDict::new_bound(py);
            d.set_item("word", &word_analysis.word)?;
            let analyses = PyList::empty_bound(py);
            for a in &word_analysis.analyses {
                let ad = PyDict::new_bound(py);
                ad.set_item("root", &a.root)?;
                ad.set_item("ending", &a.ending)?;
                ad.set_item("clitic", &a.clitic)?;
                ad.set_item("partofspeech", &a.partofspeech)?;
                ad.set_item("form", &a.form)?;
                analyses.append(ad)?;
            }
            d.set_item("analyses", analyses)?;
            list.append(d)?;
        }
        Ok(list.unbind().into())
    }

    /// Word synthesis.
    #[pyo3(signature = (lemma, form, partofspeech="", hint="", guess=true, phonetic=false))]
    fn synthesize(
        &self,
        lemma: &str,
        form: &str,
        partofspeech: &str,
        hint: &str,
        guess: bool,
        phonetic: bool,
    ) -> PyResult<Vec<String>> {
        let mut vm = self.inner.lock().unwrap();
        vm.synthesize(lemma, form, partofspeech, hint, guess, phonetic)
            .map_err(|e| pyo3::exceptions::PyRuntimeError::new_err(e.to_string()))
    }

    /// Spellcheck.
    #[pyo3(signature = (words, suggest=true))]
    fn spellcheck(&self, py: Python<'_>, words: Vec<String>, suggest: bool) -> PyResult<PyObject> {
        let word_refs: Vec<&str> = words.iter().map(|s| s.as_str()).collect();
        let mut vm = self.inner.lock().unwrap();
        let results = vm
            .spellcheck(&word_refs, suggest)
            .map_err(|e| pyo3::exceptions::PyRuntimeError::new_err(e.to_string()))?;

        let list = PyList::empty_bound(py);
        for sr in &results {
            let d = PyDict::new_bound(py);
            d.set_item("word", &sr.word)?;
            d.set_item("correct", sr.correct)?;
            let sug = PyList::empty_bound(py);
            for s in &sr.suggestions {
                sug.append(s)?;
            }
            d.set_item("suggestions", sug)?;
            list.append(d)?;
        }
        Ok(list.unbind().into())
    }

    /// Generate all 28 noun case forms.
    fn noun_forms_expander(&self, word: &str) -> PyResult<Vec<String>> {
        let mut vm = self.inner.lock().unwrap();
        estnltk_morph::noun_forms_expander(&mut vm, word)
            .map_err(|e| pyo3::exceptions::PyRuntimeError::new_err(e.to_string()))
    }

    /// Default expander (delegates to noun_forms_expander).
    fn default_expander(&self, word: &str) -> PyResult<Vec<String>> {
        let mut vm = self.inner.lock().unwrap();
        estnltk_morph::default_expander(&mut vm, word)
            .map_err(|e| pyo3::exceptions::PyRuntimeError::new_err(e.to_string()))
    }
}

/// Standalone: generate noun case forms using an RsVabamorf instance.
#[pyfunction]
pub fn rs_noun_forms_expander(vabamorf: &PyVabamorf, word: &str) -> PyResult<Vec<String>> {
    let mut vm = vabamorf.inner.lock().unwrap();
    estnltk_morph::noun_forms_expander(&mut vm, word)
        .map_err(|e| pyo3::exceptions::PyRuntimeError::new_err(e.to_string()))
}

/// Standalone: default expander using an RsVabamorf instance.
#[pyfunction]
pub fn rs_default_expander(vabamorf: &PyVabamorf, word: &str) -> PyResult<Vec<String>> {
    let mut vm = vabamorf.inner.lock().unwrap();
    estnltk_morph::default_expander(&mut vm, word)
        .map_err(|e| pyo3::exceptions::PyRuntimeError::new_err(e.to_string()))
}

/// Standalone: syllabify a word (does not require an RsVabamorf instance).
#[pyfunction]
pub fn rs_syllabify(py: Python<'_>, word: &str) -> PyResult<PyObject> {
    let syllables = vabamorf_rs::syllabify(word)
        .map_err(|e| pyo3::exceptions::PyRuntimeError::new_err(e.to_string()))?;
    let list = PyList::empty_bound(py);
    for s in &syllables {
        let d = PyDict::new_bound(py);
        d.set_item("syllable", &s.syllable)?;
        d.set_item("quantity", s.quantity)?;
        d.set_item("accent", s.accent)?;
        list.append(d)?;
    }
    Ok(list.unbind().into())
}
