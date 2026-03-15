/*
 * C API (extern "C") wrapper for the Vabamorf C++ morphological analyzer.
 * See vabamorf_ffi.h for the public interface.
 */
#include "vabamorf.h"
#include "vabamorf_ffi.h"
#include "viga.h"

#include <cstdio>
#include <cstring>
#include <stdexcept>
#include <string>
#include <vector>

/* ── Thread-local error buffer ─────────────────────────────────────── */

static thread_local char g_error_buf[2048] = {0};

static void set_error(const char* msg) {
    std::snprintf(g_error_buf, sizeof(g_error_buf), "%s", msg);
}

static void clear_error() {
    g_error_buf[0] = '\0';
}

static void catch_exception() {
    try {
        throw; // rethrow current exception
    }
    catch (const std::runtime_error& e) { set_error(e.what()); }
    catch (const std::invalid_argument& e) { set_error(e.what()); }
    catch (const std::out_of_range& e) { set_error(e.what()); }
    catch (const VEAD& e) {
        CFSAString t = e.Teade();
        set_error((const char*)t);
    }
    catch (const CFSException&) {
        set_error("CFSException: internal vabamorf error");
    }
    catch (...) { set_error("unknown exception"); }
}

/* ── Opaque handle structs ─────────────────────────────────────────── */

struct VabamorHandle {
    Vabamorf* vm;
};

struct AnalysisResultHandle {
    std::vector<WordAnalysis> data;
};

struct SpellResultHandle {
    std::vector<SpellingResults> data;
};

struct SynthResultHandle {
    StringVector data;
};

struct SyllResultHandle {
    Syllables data;
};

/* ── Library lifecycle ─────────────────────────────────────────────── */

extern "C" int vabamorf_init(void) {
    clear_error();
    try {
        if (FSCInit()) return 0;
        set_error("FSCInit failed");
    } catch (...) { catch_exception(); }
    return -1;
}

extern "C" void vabamorf_terminate(void) {
    FSCTerminate();
}

extern "C" const char* vabamorf_last_error(void) {
    return g_error_buf;
}

/* ── Vabamorf instance ─────────────────────────────────────────────── */

extern "C" VabamorHandle* vabamorf_new(const char* lex_path, const char* disamb_lex_path) {
    clear_error();
    try {
        FSCInit(); // idempotent safety
        VabamorHandle* h = new VabamorHandle();
        h->vm = new Vabamorf(std::string(lex_path), std::string(disamb_lex_path));
        return h;
    } catch (...) { catch_exception(); }
    return nullptr;
}

extern "C" void vabamorf_free(VabamorHandle* handle) {
    if (handle) {
        delete handle->vm;
        delete handle;
    }
}

/* ── Morphological analysis ────────────────────────────────────────── */

extern "C" AnalysisResultHandle* vabamorf_analyze(
    VabamorHandle* handle,
    const char** words, int word_count,
    int disambiguate, int guess, int phonetic, int propername, int stem)
{
    clear_error();
    try {
        StringVector sentence;
        sentence.reserve(word_count);
        for (int i = 0; i < word_count; ++i) {
            sentence.push_back(std::string(words[i]));
        }
        AnalysisResultHandle* r = new AnalysisResultHandle();
        r->data = handle->vm->analyze(sentence,
            disambiguate != 0, guess != 0, phonetic != 0, propername != 0, stem != 0);
        return r;
    } catch (...) { catch_exception(); }
    return nullptr;
}

extern "C" int analysis_result_word_count(const AnalysisResultHandle* result) {
    if (!result) return 0;
    return (int)result->data.size();
}

extern "C" const char* analysis_result_word(const AnalysisResultHandle* result, int word_idx) {
    if (!result || word_idx < 0 || word_idx >= (int)result->data.size()) return "";
    return result->data[word_idx].first.c_str();
}

extern "C" int analysis_result_analysis_count(const AnalysisResultHandle* result, int word_idx) {
    if (!result || word_idx < 0 || word_idx >= (int)result->data.size()) return 0;
    return (int)result->data[word_idx].second.size();
}

extern "C" const char* analysis_result_root(const AnalysisResultHandle* result, int word_idx, int analysis_idx) {
    if (!result || word_idx < 0 || word_idx >= (int)result->data.size()) return "";
    const auto& analyses = result->data[word_idx].second;
    if (analysis_idx < 0 || analysis_idx >= (int)analyses.size()) return "";
    return analyses[analysis_idx].root.c_str();
}

extern "C" const char* analysis_result_ending(const AnalysisResultHandle* result, int word_idx, int analysis_idx) {
    if (!result || word_idx < 0 || word_idx >= (int)result->data.size()) return "";
    const auto& analyses = result->data[word_idx].second;
    if (analysis_idx < 0 || analysis_idx >= (int)analyses.size()) return "";
    return analyses[analysis_idx].ending.c_str();
}

extern "C" const char* analysis_result_clitic(const AnalysisResultHandle* result, int word_idx, int analysis_idx) {
    if (!result || word_idx < 0 || word_idx >= (int)result->data.size()) return "";
    const auto& analyses = result->data[word_idx].second;
    if (analysis_idx < 0 || analysis_idx >= (int)analyses.size()) return "";
    return analyses[analysis_idx].clitic.c_str();
}

extern "C" const char* analysis_result_partofspeech(const AnalysisResultHandle* result, int word_idx, int analysis_idx) {
    if (!result || word_idx < 0 || word_idx >= (int)result->data.size()) return "";
    const auto& analyses = result->data[word_idx].second;
    if (analysis_idx < 0 || analysis_idx >= (int)analyses.size()) return "";
    return analyses[analysis_idx].partofspeech.c_str();
}

extern "C" const char* analysis_result_form(const AnalysisResultHandle* result, int word_idx, int analysis_idx) {
    if (!result || word_idx < 0 || word_idx >= (int)result->data.size()) return "";
    const auto& analyses = result->data[word_idx].second;
    if (analysis_idx < 0 || analysis_idx >= (int)analyses.size()) return "";
    return analyses[analysis_idx].form.c_str();
}

extern "C" void analysis_result_free(AnalysisResultHandle* result) {
    delete result;
}

/* ── Spellcheck ────────────────────────────────────────────────────── */

extern "C" SpellResultHandle* vabamorf_spellcheck(
    VabamorHandle* handle,
    const char** words, int word_count,
    int suggest)
{
    clear_error();
    try {
        StringVector sentence;
        sentence.reserve(word_count);
        for (int i = 0; i < word_count; ++i) {
            sentence.push_back(std::string(words[i]));
        }
        SpellResultHandle* r = new SpellResultHandle();
        r->data = handle->vm->spellcheck(sentence, suggest != 0);
        return r;
    } catch (...) { catch_exception(); }
    return nullptr;
}

extern "C" int spell_result_count(const SpellResultHandle* result) {
    if (!result) return 0;
    return (int)result->data.size();
}

extern "C" const char* spell_result_word(const SpellResultHandle* result, int idx) {
    if (!result || idx < 0 || idx >= (int)result->data.size()) return "";
    return result->data[idx].word.c_str();
}

extern "C" int spell_result_correct(const SpellResultHandle* result, int idx) {
    if (!result || idx < 0 || idx >= (int)result->data.size()) return 0;
    return result->data[idx].spelling ? 1 : 0;
}

extern "C" int spell_result_suggestion_count(const SpellResultHandle* result, int idx) {
    if (!result || idx < 0 || idx >= (int)result->data.size()) return 0;
    return (int)result->data[idx].suggestions.size();
}

extern "C" const char* spell_result_suggestion(const SpellResultHandle* result, int idx, int sug_idx) {
    if (!result || idx < 0 || idx >= (int)result->data.size()) return "";
    const auto& suggestions = result->data[idx].suggestions;
    if (sug_idx < 0 || sug_idx >= (int)suggestions.size()) return "";
    return suggestions[sug_idx].c_str();
}

extern "C" void spell_result_free(SpellResultHandle* result) {
    delete result;
}

/* ── Synthesis ─────────────────────────────────────────────────────── */

extern "C" SynthResultHandle* vabamorf_synthesize(
    VabamorHandle* handle,
    const char* lemma, const char* form,
    const char* partofspeech, const char* hint,
    int guess, int phonetic)
{
    clear_error();
    try {
        SynthResultHandle* r = new SynthResultHandle();
        r->data = handle->vm->synthesize(
            std::string(lemma), std::string(form),
            std::string(partofspeech ? partofspeech : ""),
            std::string(hint ? hint : ""),
            guess != 0, phonetic != 0);
        return r;
    } catch (...) { catch_exception(); }
    return nullptr;
}

extern "C" int synth_result_count(const SynthResultHandle* result) {
    if (!result) return 0;
    return (int)result->data.size();
}

extern "C" const char* synth_result_word(const SynthResultHandle* result, int idx) {
    if (!result || idx < 0 || idx >= (int)result->data.size()) return "";
    return result->data[idx].c_str();
}

extern "C" void synth_result_free(SynthResultHandle* result) {
    delete result;
}

/* ── Syllabification ───────────────────────────────────────────────── */

extern "C" SyllResultHandle* vabamorf_syllabify(const char* word) {
    clear_error();
    try {
        FSCInit(); // idempotent safety
        SyllResultHandle* r = new SyllResultHandle();
        r->data = syllabify(std::string(word));
        return r;
    } catch (...) { catch_exception(); }
    return nullptr;
}

extern "C" int syll_result_count(const SyllResultHandle* result) {
    if (!result) return 0;
    return (int)result->data.size();
}

extern "C" const char* syll_result_syllable(const SyllResultHandle* result, int idx) {
    if (!result || idx < 0 || idx >= (int)result->data.size()) return "";
    return result->data[idx].syllable.c_str();
}

extern "C" int syll_result_quantity(const SyllResultHandle* result, int idx) {
    if (!result || idx < 0 || idx >= (int)result->data.size()) return 0;
    return result->data[idx].quantity;
}

extern "C" int syll_result_accent(const SyllResultHandle* result, int idx) {
    if (!result || idx < 0 || idx >= (int)result->data.size()) return 0;
    return result->data[idx].accent;
}

extern "C" void syll_result_free(SyllResultHandle* result) {
    delete result;
}
