//! Criterion benchmarks for Vabamorf morphological analysis, spellcheck,
//! synthesis, and syllabification.

use criterion::{black_box, criterion_group, criterion_main, Criterion};
use std::path::Path;
use vabamorf_rs::Vabamorf;

/// Short Estonian sentence (6 words).
const SENTENCE_SHORT: &[&str] = &["Tere", "hommikust", "kallis", "sõber", "ja", "naaber"];

/// Longer Estonian paragraph (49 words) — representative real-world input.
const PARAGRAPH: &[&str] = &[
    "Eesti", "Vabariik", "on", "demokraatlik", "riik", "Põhja-Euroopas",
    "Eesti", "piirneb", "põhjas", "ja", "läänes", "Läänemeraga", "lõunas",
    "Lätiga", "ja", "idas", "Venemaaga", "Eesti", "pindala", "on", "45339",
    "ruutkilomeetrit", "ja", "rahvaarv", "on", "ligikaudu", "miljonit",
    "Pealinn", "ja", "suurim", "linn", "on", "Tallinn", "Eesti", "on",
    "Euroopa", "Liidu", "NATO", "Euroopa", "Nõukogu", "OECD", "ja",
    "paljude", "teiste", "rahvusvaheliste", "organisatsioonide", "liige",
    "Riigi", "ametlik",
];

fn dct_dir() -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../vabamorf-cpp/dct")
}

fn get_vm() -> Vabamorf {
    Vabamorf::from_dct_dir(&dct_dir()).expect("Failed to load Vabamorf dicts")
}

// --- Morphological analysis ---

fn bench_vabamorf_analyze(c: &mut Criterion) {
    let mut group = c.benchmark_group("vabamorf_analyze");
    let mut vm = get_vm();

    // Short sentence, raw (no disambiguation)
    group.bench_function("raw/sentence_6w", |b| {
        b.iter(|| {
            let result = vm
                .analyze(black_box(SENTENCE_SHORT), false, true, false, true, false)
                .unwrap();
            black_box(&result);
        });
    });

    // Short sentence, disambiguated
    group.bench_function("disambiguated/sentence_6w", |b| {
        b.iter(|| {
            let result = vm
                .analyze(black_box(SENTENCE_SHORT), true, true, false, true, false)
                .unwrap();
            black_box(&result);
        });
    });

    // Paragraph, raw
    group.bench_function("raw/paragraph_49w", |b| {
        b.iter(|| {
            let result = vm
                .analyze(black_box(PARAGRAPH), false, true, false, true, false)
                .unwrap();
            black_box(&result);
        });
    });

    // Paragraph, disambiguated
    group.bench_function("disambiguated/paragraph_49w", |b| {
        b.iter(|| {
            let result = vm
                .analyze(black_box(PARAGRAPH), true, true, false, true, false)
                .unwrap();
            black_box(&result);
        });
    });

    group.finish();
}

// --- Spellcheck ---

fn bench_vabamorf_spellcheck(c: &mut Criterion) {
    let mut group = c.benchmark_group("vabamorf_spellcheck");
    let mut vm = get_vm();

    let correct_words: &[&str] = &["tere", "maailm", "keel", "eesti", "riik", "linn"];
    let misspelled: &[&str] = &["teree", "maalm", "kell", "esti", "riikk", "lin"];

    // Check correct words (no suggestions needed)
    group.bench_function("check/correct_6w", |b| {
        b.iter(|| {
            let result = vm.spellcheck(black_box(correct_words), false).unwrap();
            black_box(&result);
        });
    });

    // Check misspelled words without suggestions
    group.bench_function("check/misspelled_6w", |b| {
        b.iter(|| {
            let result = vm.spellcheck(black_box(misspelled), false).unwrap();
            black_box(&result);
        });
    });

    // Check misspelled words WITH suggestions (expensive)
    group.bench_function("suggest/misspelled_6w", |b| {
        b.iter(|| {
            let result = vm.spellcheck(black_box(misspelled), true).unwrap();
            black_box(&result);
        });
    });

    group.finish();
}

// --- Synthesis ---

fn bench_vabamorf_synthesize(c: &mut Criterion) {
    let mut group = c.benchmark_group("vabamorf_synthesize");
    let mut vm = get_vm();

    let test_cases: &[(&str, &str, &str)] = &[
        ("maja", "sg g", "S"),
        ("maja", "pl p", "S"),
        ("tegema", "da", "V"),
        ("suur", "sg komp", "A"),
        ("inimene", "pl g", "S"),
    ];

    group.bench_function("5_calls", |b| {
        b.iter(|| {
            for &(lemma, form, pos) in black_box(test_cases) {
                let result = vm.synthesize(lemma, form, pos, "", true, false).unwrap();
                black_box(&result);
            }
        });
    });

    // Single synthesis call
    group.bench_function("single_noun", |b| {
        b.iter(|| {
            let result = vm
                .synthesize(black_box("maja"), black_box("sg g"), "S", "", true, false)
                .unwrap();
            black_box(&result);
        });
    });

    group.finish();
}

// --- Syllabification ---

fn bench_vabamorf_syllabify(c: &mut Criterion) {
    let mut group = c.benchmark_group("vabamorf_syllabify");

    // Single word
    group.bench_function("single_word", |b| {
        b.iter(|| {
            let result = vabamorf_rs::syllabify(black_box("organisatsioonide")).unwrap();
            black_box(&result);
        });
    });

    // Batch of words from paragraph
    let words_49: Vec<&str> = PARAGRAPH.to_vec();
    group.bench_function("49_words", |b| {
        b.iter(|| {
            for word in black_box(&words_49) {
                let result = vabamorf_rs::syllabify(word).unwrap();
                black_box(&result);
            }
        });
    });

    group.finish();
}

criterion_group!(
    vabamorf_bench_group,
    bench_vabamorf_analyze,
    bench_vabamorf_spellcheck,
    bench_vabamorf_synthesize,
    bench_vabamorf_syllabify,
);

criterion_main!(vabamorf_bench_group);
