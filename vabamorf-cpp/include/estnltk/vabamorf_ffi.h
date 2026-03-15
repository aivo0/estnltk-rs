/*
 * C API (extern "C") wrapper for the Vabamorf C++ morphological analyzer.
 * Provides an FFI-friendly interface for use from Rust and other languages.
 *
 * All strings are UTF-8 encoded const char*.
 * Opaque handles own their data until explicitly freed.
 * Error details available via vabamorf_last_error() (thread-local).
 */
#ifndef VABAMORF_FFI_H
#define VABAMORF_FFI_H

#ifdef __cplusplus
extern "C" {
#endif

/* ── Library lifecycle ─────────────────────────────────────────────── */

/** Initialize the FSC subsystem. Idempotent. Returns 0 on success, -1 on error. */
int vabamorf_init(void);

/** Terminate the FSC subsystem. */
void vabamorf_terminate(void);

/** Return the last error message (thread-local). Empty string if no error. */
const char* vabamorf_last_error(void);

/* ── Vabamorf instance ─────────────────────────────────────────────── */

typedef struct VabamorHandle VabamorHandle;

/**
 * Create a new Vabamorf instance.
 * @param lex_path       Path to the morphological lexicon (et.dct).
 * @param disamb_lex_path Path to the disambiguation lexicon (et3.dct).
 * @return Handle, or NULL on error (check vabamorf_last_error).
 */
VabamorHandle* vabamorf_new(const char* lex_path, const char* disamb_lex_path);

/** Free a Vabamorf instance. NULL-safe. */
void vabamorf_free(VabamorHandle* handle);

/* ── Morphological analysis ────────────────────────────────────────── */

typedef struct AnalysisResultHandle AnalysisResultHandle;

/**
 * Analyze a sentence (array of words).
 * @return Result handle, or NULL on error.
 */
AnalysisResultHandle* vabamorf_analyze(
    VabamorHandle* handle,
    const char** words, int word_count,
    int disambiguate, int guess, int phonetic, int propername, int stem);

int         analysis_result_word_count(const AnalysisResultHandle* result);
const char* analysis_result_word(const AnalysisResultHandle* result, int word_idx);
int         analysis_result_analysis_count(const AnalysisResultHandle* result, int word_idx);
const char* analysis_result_root(const AnalysisResultHandle* result, int word_idx, int analysis_idx);
const char* analysis_result_ending(const AnalysisResultHandle* result, int word_idx, int analysis_idx);
const char* analysis_result_clitic(const AnalysisResultHandle* result, int word_idx, int analysis_idx);
const char* analysis_result_partofspeech(const AnalysisResultHandle* result, int word_idx, int analysis_idx);
const char* analysis_result_form(const AnalysisResultHandle* result, int word_idx, int analysis_idx);
void        analysis_result_free(AnalysisResultHandle* result);

/* ── Spellcheck ────────────────────────────────────────────────────── */

typedef struct SpellResultHandle SpellResultHandle;

/**
 * Spellcheck a sentence.
 * @return Result handle, or NULL on error.
 */
SpellResultHandle* vabamorf_spellcheck(
    VabamorHandle* handle,
    const char** words, int word_count,
    int suggest);

int         spell_result_count(const SpellResultHandle* result);
const char* spell_result_word(const SpellResultHandle* result, int idx);
int         spell_result_correct(const SpellResultHandle* result, int idx);
int         spell_result_suggestion_count(const SpellResultHandle* result, int idx);
const char* spell_result_suggestion(const SpellResultHandle* result, int idx, int sug_idx);
void        spell_result_free(SpellResultHandle* result);

/* ── Synthesis ─────────────────────────────────────────────────────── */

typedef struct SynthResultHandle SynthResultHandle;

/**
 * Synthesize word forms.
 * @return Result handle, or NULL on error.
 */
SynthResultHandle* vabamorf_synthesize(
    VabamorHandle* handle,
    const char* lemma, const char* form,
    const char* partofspeech, const char* hint,
    int guess, int phonetic);

int         synth_result_count(const SynthResultHandle* result);
const char* synth_result_word(const SynthResultHandle* result, int idx);
void        synth_result_free(SynthResultHandle* result);

/* ── Syllabification ───────────────────────────────────────────────── */

typedef struct SyllResultHandle SyllResultHandle;

/** Syllabify a single word (standalone, no Vabamorf handle needed). */
SyllResultHandle* vabamorf_syllabify(const char* word);

int         syll_result_count(const SyllResultHandle* result);
const char* syll_result_syllable(const SyllResultHandle* result, int idx);
int         syll_result_quantity(const SyllResultHandle* result, int idx);
int         syll_result_accent(const SyllResultHandle* result, int idx);
void        syll_result_free(SyllResultHandle* result);

#ifdef __cplusplus
}
#endif

#endif /* VABAMORF_FFI_H */
