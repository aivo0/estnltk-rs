//! Criterion benchmarks for estnltk-morph noun forms expansion.

use criterion::{black_box, criterion_group, criterion_main, Criterion};
use std::collections::HashMap;
use std::path::Path;

use estnltk_morph::{noun_forms_expander, expand_rules};
use estnltk_taggers::SubstringRule;
use vabamorf_rs::Vabamorf;

fn dct_dir() -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../vabamorf-cpp/dct")
}

fn get_vm() -> Vabamorf {
    Vabamorf::from_dct_dir(&dct_dir()).expect("Failed to load Vabamorf dicts")
}

/// Common Estonian nouns for expansion benchmarks.
const TEST_NOUNS: &[&str] = &[
    "maja", "inimene", "keel", "riik", "linn", "mets", "saar", "järv",
];

fn bench_noun_expander(c: &mut Criterion) {
    let mut group = c.benchmark_group("noun_forms_expander");
    let mut vm = get_vm();

    // Single noun expansion (28 forms)
    group.bench_function("single_noun", |b| {
        b.iter(|| {
            let result = noun_forms_expander(&mut vm, black_box("maja")).unwrap();
            black_box(&result);
        });
    });

    // Batch of 8 nouns
    group.bench_function("8_nouns", |b| {
        b.iter(|| {
            for &noun in black_box(TEST_NOUNS) {
                let result = noun_forms_expander(&mut vm, noun).unwrap();
                black_box(&result);
            }
        });
    });

    group.finish();
}

fn bench_expand_rules(c: &mut Criterion) {
    let mut group = c.benchmark_group("expand_rules");
    let mut vm = get_vm();

    let rules: Vec<SubstringRule> = TEST_NOUNS
        .iter()
        .map(|noun| SubstringRule::new(noun, HashMap::new(), 0, 0))
        .collect();

    // Expand 8 rules (each noun -> up to 28 forms)
    group.bench_function("8_rules", |b| {
        b.iter(|| {
            let result =
                expand_rules(black_box(rules.clone()), "noun_forms", &mut vm, false).unwrap();
            black_box(&result);
        });
    });

    // With lowercase
    group.bench_function("8_rules_lowercase", |b| {
        b.iter(|| {
            let result =
                expand_rules(black_box(rules.clone()), "noun_forms", &mut vm, true).unwrap();
            black_box(&result);
        });
    });

    group.finish();
}

criterion_group!(
    morph_bench_group,
    bench_noun_expander,
    bench_expand_rules,
);

criterion_main!(morph_bench_group);
