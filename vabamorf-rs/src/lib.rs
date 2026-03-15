//! Safe Rust bindings to the Vabamorf Estonian morphological analyzer.
//!
//! Vabamorf provides morphological analysis, disambiguation, spellchecking,
//! word synthesis, and syllabification for the Estonian language.
//!
//! # Example
//! ```no_run
//! use std::path::Path;
//! use vabamorf_rs::Vabamorf;
//!
//! let mut vm = Vabamorf::from_dct_dir(Path::new("path/to/dct")).unwrap();
//! let results = vm.analyze(&["tere", "maailm"], true, true, false, true, false).unwrap();
//! for word in &results {
//!     println!("{}: {:?}", word.word, word.analyses);
//! }
//! ```

use std::ffi::{CStr, CString};
use std::fmt;
use std::os::raw::c_char;
use std::path::Path;

/// Error type for Vabamorf operations.
#[derive(Debug, Clone)]
pub struct VabamorError(String);

impl fmt::Display for VabamorError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "VabamorError: {}", self.0)
    }
}

impl std::error::Error for VabamorError {}

/// A single morphological analysis of a word.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Analysis {
    pub root: String,
    pub ending: String,
    pub clitic: String,
    pub partofspeech: String,
    pub form: String,
}

/// A word together with all its possible morphological analyses.
#[derive(Debug, Clone)]
pub struct WordAnalysis {
    pub word: String,
    pub analyses: Vec<Analysis>,
}

/// Result of a spellcheck operation for a single word.
#[derive(Debug, Clone)]
pub struct SpellingResult {
    pub word: String,
    pub correct: bool,
    pub suggestions: Vec<String>,
}

/// A single syllable with quantity and accent information.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Syllable {
    pub syllable: String,
    /// Syllable quantity (välde): 1, 2, or 3.
    pub quantity: i32,
    /// Accent/stress (rõhk).
    pub accent: i32,
}

/// Read a C string pointer into a Rust String. Returns empty string for null.
unsafe fn cstr_to_string(ptr: *const c_char) -> String {
    if ptr.is_null() {
        String::new()
    } else {
        CStr::from_ptr(ptr).to_string_lossy().into_owned()
    }
}

/// Get the last error from the FFI layer.
fn last_error() -> String {
    unsafe { cstr_to_string(vabamorf_sys::vabamorf_last_error()) }
}

/// Initialize the underlying FSC library.
///
/// This is called automatically by [`Vabamorf::new`], but can be called
/// explicitly if needed. Idempotent.
pub fn init() -> Result<(), VabamorError> {
    let rc = unsafe { vabamorf_sys::vabamorf_init() };
    if rc == 0 {
        Ok(())
    } else {
        Err(VabamorError(last_error()))
    }
}

/// Terminate the underlying FSC library.
pub fn terminate() {
    unsafe { vabamorf_sys::vabamorf_terminate() }
}

/// Syllabify a single word.
///
/// This is a standalone function that does not require a [`Vabamorf`] instance.
pub fn syllabify(word: &str) -> Result<Vec<Syllable>, VabamorError> {
    let c_word = CString::new(word).map_err(|e| VabamorError(e.to_string()))?;
    let handle = unsafe { vabamorf_sys::vabamorf_syllabify(c_word.as_ptr()) };
    if handle.is_null() {
        return Err(VabamorError(last_error()));
    }

    let count = unsafe { vabamorf_sys::syll_result_count(handle) };
    let mut syllables = Vec::with_capacity(count as usize);
    for i in 0..count {
        syllables.push(Syllable {
            syllable: unsafe { cstr_to_string(vabamorf_sys::syll_result_syllable(handle, i)) },
            quantity: unsafe { vabamorf_sys::syll_result_quantity(handle, i) },
            accent: unsafe { vabamorf_sys::syll_result_accent(handle, i) },
        });
    }
    unsafe { vabamorf_sys::syll_result_free(handle) };
    Ok(syllables)
}

/// Estonian morphological analyzer.
///
/// Wraps the C++ Vabamorf library. Each instance loads the morphological
/// lexicon and disambiguation dictionary from `.dct` files.
///
/// # Thread Safety
///
/// `Vabamorf` is `Send` but not `Sync`. The underlying C++ code mutates
/// internal state during analysis. To use from multiple threads, either
/// create one instance per thread or wrap in a `Mutex`.
pub struct Vabamorf {
    handle: *mut vabamorf_sys::VabamorHandle,
}

// Safe to move between threads, but not to share references
unsafe impl Send for Vabamorf {}

impl Vabamorf {
    /// Create a new Vabamorf instance with explicit dictionary paths.
    ///
    /// - `lex_path`: path to the morphological lexicon file (e.g., `et.dct`)
    /// - `disamb_lex_path`: path to the disambiguation lexicon file (e.g., `et3.dct`)
    pub fn new(lex_path: &str, disamb_lex_path: &str) -> Result<Self, VabamorError> {
        let c_lex = CString::new(lex_path).map_err(|e| VabamorError(e.to_string()))?;
        let c_disamb = CString::new(disamb_lex_path).map_err(|e| VabamorError(e.to_string()))?;

        let handle = unsafe { vabamorf_sys::vabamorf_new(c_lex.as_ptr(), c_disamb.as_ptr()) };
        if handle.is_null() {
            return Err(VabamorError(last_error()));
        }
        Ok(Vabamorf { handle })
    }

    /// Create a new Vabamorf instance from a directory containing `et.dct` and `et3.dct`.
    pub fn from_dct_dir(dir: &Path) -> Result<Self, VabamorError> {
        let lex = dir.join("et.dct");
        let disamb = dir.join("et3.dct");
        if !lex.exists() {
            return Err(VabamorError(format!(
                "Lexicon file not found: {}",
                lex.display()
            )));
        }
        if !disamb.exists() {
            return Err(VabamorError(format!(
                "Disambiguation lexicon not found: {}",
                disamb.display()
            )));
        }
        let lex_str = lex.to_str().ok_or_else(|| VabamorError("Invalid UTF-8 in lex path".into()))?;
        let disamb_str = disamb.to_str().ok_or_else(|| VabamorError("Invalid UTF-8 in disamb path".into()))?;
        Self::new(lex_str, disamb_str)
    }

    /// Perform morphological analysis on a sentence (slice of words).
    ///
    /// - `disambiguate`: reduce ambiguity using the disambiguation model
    /// - `guess`: attempt to analyze unknown words
    /// - `phonetic`: include phonetic markup in roots
    /// - `propername`: perform additional proper name analysis
    /// - `stem`: return stems instead of roots
    pub fn analyze(
        &mut self,
        sentence: &[&str],
        disambiguate: bool,
        guess: bool,
        phonetic: bool,
        propername: bool,
        stem: bool,
    ) -> Result<Vec<WordAnalysis>, VabamorError> {
        let c_words: Vec<CString> = sentence
            .iter()
            .map(|w| CString::new(*w).map_err(|e| VabamorError(e.to_string())))
            .collect::<Result<_, _>>()?;
        let c_ptrs: Vec<*const c_char> = c_words.iter().map(|s| s.as_ptr()).collect();

        let result = unsafe {
            vabamorf_sys::vabamorf_analyze(
                self.handle,
                c_ptrs.as_ptr(),
                c_ptrs.len() as i32,
                disambiguate as i32,
                guess as i32,
                phonetic as i32,
                propername as i32,
                stem as i32,
            )
        };
        if result.is_null() {
            return Err(VabamorError(last_error()));
        }

        let word_count = unsafe { vabamorf_sys::analysis_result_word_count(result) };
        let mut words = Vec::with_capacity(word_count as usize);
        for wi in 0..word_count {
            let word = unsafe { cstr_to_string(vabamorf_sys::analysis_result_word(result, wi)) };
            let analysis_count =
                unsafe { vabamorf_sys::analysis_result_analysis_count(result, wi) };
            let mut analyses = Vec::with_capacity(analysis_count as usize);
            for ai in 0..analysis_count {
                analyses.push(Analysis {
                    root: unsafe {
                        cstr_to_string(vabamorf_sys::analysis_result_root(result, wi, ai))
                    },
                    ending: unsafe {
                        cstr_to_string(vabamorf_sys::analysis_result_ending(result, wi, ai))
                    },
                    clitic: unsafe {
                        cstr_to_string(vabamorf_sys::analysis_result_clitic(result, wi, ai))
                    },
                    partofspeech: unsafe {
                        cstr_to_string(vabamorf_sys::analysis_result_partofspeech(result, wi, ai))
                    },
                    form: unsafe {
                        cstr_to_string(vabamorf_sys::analysis_result_form(result, wi, ai))
                    },
                });
            }
            words.push(WordAnalysis { word, analyses });
        }
        unsafe { vabamorf_sys::analysis_result_free(result) };
        Ok(words)
    }

    /// Spellcheck a sentence.
    ///
    /// - `suggest`: if true, include spelling suggestions for misspelled words
    pub fn spellcheck(
        &mut self,
        sentence: &[&str],
        suggest: bool,
    ) -> Result<Vec<SpellingResult>, VabamorError> {
        let c_words: Vec<CString> = sentence
            .iter()
            .map(|w| CString::new(*w).map_err(|e| VabamorError(e.to_string())))
            .collect::<Result<_, _>>()?;
        let c_ptrs: Vec<*const c_char> = c_words.iter().map(|s| s.as_ptr()).collect();

        let result = unsafe {
            vabamorf_sys::vabamorf_spellcheck(
                self.handle,
                c_ptrs.as_ptr(),
                c_ptrs.len() as i32,
                suggest as i32,
            )
        };
        if result.is_null() {
            return Err(VabamorError(last_error()));
        }

        let count = unsafe { vabamorf_sys::spell_result_count(result) };
        let mut results = Vec::with_capacity(count as usize);
        for i in 0..count {
            let word = unsafe { cstr_to_string(vabamorf_sys::spell_result_word(result, i)) };
            let correct = unsafe { vabamorf_sys::spell_result_correct(result, i) != 0 };
            let sug_count =
                unsafe { vabamorf_sys::spell_result_suggestion_count(result, i) };
            let mut suggestions = Vec::with_capacity(sug_count as usize);
            for si in 0..sug_count {
                suggestions.push(unsafe {
                    cstr_to_string(vabamorf_sys::spell_result_suggestion(result, i, si))
                });
            }
            results.push(SpellingResult {
                word,
                correct,
                suggestions,
            });
        }
        unsafe { vabamorf_sys::spell_result_free(result) };
        Ok(results)
    }

    /// Synthesize word forms from a lemma.
    ///
    /// - `lemma`: base form of the word
    /// - `form`: target grammatical form
    /// - `partofspeech`: part of speech filter (empty string for any)
    /// - `hint`: hint for synthesis (empty string for none)
    /// - `guess`: use heuristics for unknown words
    /// - `phonetic`: add phonetic markup
    pub fn synthesize(
        &mut self,
        lemma: &str,
        form: &str,
        partofspeech: &str,
        hint: &str,
        guess: bool,
        phonetic: bool,
    ) -> Result<Vec<String>, VabamorError> {
        let c_lemma = CString::new(lemma).map_err(|e| VabamorError(e.to_string()))?;
        let c_form = CString::new(form).map_err(|e| VabamorError(e.to_string()))?;
        let c_pos = CString::new(partofspeech).map_err(|e| VabamorError(e.to_string()))?;
        let c_hint = CString::new(hint).map_err(|e| VabamorError(e.to_string()))?;

        let result = unsafe {
            vabamorf_sys::vabamorf_synthesize(
                self.handle,
                c_lemma.as_ptr(),
                c_form.as_ptr(),
                c_pos.as_ptr(),
                c_hint.as_ptr(),
                guess as i32,
                phonetic as i32,
            )
        };
        if result.is_null() {
            return Err(VabamorError(last_error()));
        }

        let count = unsafe { vabamorf_sys::synth_result_count(result) };
        let mut words = Vec::with_capacity(count as usize);
        for i in 0..count {
            words.push(unsafe { cstr_to_string(vabamorf_sys::synth_result_word(result, i)) });
        }
        unsafe { vabamorf_sys::synth_result_free(result) };
        Ok(words)
    }
}

impl Drop for Vabamorf {
    fn drop(&mut self) {
        if !self.handle.is_null() {
            unsafe { vabamorf_sys::vabamorf_free(self.handle) };
        }
    }
}
