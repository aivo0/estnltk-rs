//! Raw FFI bindings to the Vabamorf Estonian morphological analyzer.
//!
//! This crate provides low-level `unsafe` bindings to the C shim layer
//! over the C++ Vabamorf library. Prefer using the safe `vabamorf-rs` crate.

#![allow(non_camel_case_types)]

use std::os::raw::{c_char, c_int};

// Opaque handle types
#[repr(C)]
pub struct VabamorHandle {
    _private: [u8; 0],
}

#[repr(C)]
pub struct AnalysisResultHandle {
    _private: [u8; 0],
}

#[repr(C)]
pub struct SpellResultHandle {
    _private: [u8; 0],
}

#[repr(C)]
pub struct SynthResultHandle {
    _private: [u8; 0],
}

#[repr(C)]
pub struct SyllResultHandle {
    _private: [u8; 0],
}

extern "C" {
    // ── Library lifecycle ──────────────────────────────────────────
    pub fn vabamorf_init() -> c_int;
    pub fn vabamorf_terminate();
    pub fn vabamorf_last_error() -> *const c_char;

    // ── Vabamorf instance ──────────────────────────────────────────
    pub fn vabamorf_new(
        lex_path: *const c_char,
        disamb_lex_path: *const c_char,
    ) -> *mut VabamorHandle;

    pub fn vabamorf_free(handle: *mut VabamorHandle);

    // ── Morphological analysis ─────────────────────────────────────
    pub fn vabamorf_analyze(
        handle: *mut VabamorHandle,
        words: *const *const c_char,
        word_count: c_int,
        disambiguate: c_int,
        guess: c_int,
        phonetic: c_int,
        propername: c_int,
        stem: c_int,
    ) -> *mut AnalysisResultHandle;

    pub fn analysis_result_word_count(result: *const AnalysisResultHandle) -> c_int;
    pub fn analysis_result_word(result: *const AnalysisResultHandle, word_idx: c_int)
        -> *const c_char;
    pub fn analysis_result_analysis_count(
        result: *const AnalysisResultHandle,
        word_idx: c_int,
    ) -> c_int;
    pub fn analysis_result_root(
        result: *const AnalysisResultHandle,
        word_idx: c_int,
        analysis_idx: c_int,
    ) -> *const c_char;
    pub fn analysis_result_ending(
        result: *const AnalysisResultHandle,
        word_idx: c_int,
        analysis_idx: c_int,
    ) -> *const c_char;
    pub fn analysis_result_clitic(
        result: *const AnalysisResultHandle,
        word_idx: c_int,
        analysis_idx: c_int,
    ) -> *const c_char;
    pub fn analysis_result_partofspeech(
        result: *const AnalysisResultHandle,
        word_idx: c_int,
        analysis_idx: c_int,
    ) -> *const c_char;
    pub fn analysis_result_form(
        result: *const AnalysisResultHandle,
        word_idx: c_int,
        analysis_idx: c_int,
    ) -> *const c_char;
    pub fn analysis_result_free(result: *mut AnalysisResultHandle);

    // ── Spellcheck ─────────────────────────────────────────────────
    pub fn vabamorf_spellcheck(
        handle: *mut VabamorHandle,
        words: *const *const c_char,
        word_count: c_int,
        suggest: c_int,
    ) -> *mut SpellResultHandle;

    pub fn spell_result_count(result: *const SpellResultHandle) -> c_int;
    pub fn spell_result_word(result: *const SpellResultHandle, idx: c_int) -> *const c_char;
    pub fn spell_result_correct(result: *const SpellResultHandle, idx: c_int) -> c_int;
    pub fn spell_result_suggestion_count(result: *const SpellResultHandle, idx: c_int) -> c_int;
    pub fn spell_result_suggestion(
        result: *const SpellResultHandle,
        idx: c_int,
        sug_idx: c_int,
    ) -> *const c_char;
    pub fn spell_result_free(result: *mut SpellResultHandle);

    // ── Synthesis ──────────────────────────────────────────────────
    pub fn vabamorf_synthesize(
        handle: *mut VabamorHandle,
        lemma: *const c_char,
        form: *const c_char,
        partofspeech: *const c_char,
        hint: *const c_char,
        guess: c_int,
        phonetic: c_int,
    ) -> *mut SynthResultHandle;

    pub fn synth_result_count(result: *const SynthResultHandle) -> c_int;
    pub fn synth_result_word(result: *const SynthResultHandle, idx: c_int) -> *const c_char;
    pub fn synth_result_free(result: *mut SynthResultHandle);

    // ── Syllabification ────────────────────────────────────────────
    pub fn vabamorf_syllabify(word: *const c_char) -> *mut SyllResultHandle;

    pub fn syll_result_count(result: *const SyllResultHandle) -> c_int;
    pub fn syll_result_syllable(result: *const SyllResultHandle, idx: c_int) -> *const c_char;
    pub fn syll_result_quantity(result: *const SyllResultHandle, idx: c_int) -> c_int;
    pub fn syll_result_accent(result: *const SyllResultHandle, idx: c_int) -> c_int;
    pub fn syll_result_free(result: *mut SyllResultHandle);
}
