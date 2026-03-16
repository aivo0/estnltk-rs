//! Criterion benchmarks for RegexTagger, SubstringTagger, SpanTagger, and PhraseTagger.
//!
//! Measures throughput across varying text sizes, pattern counts,
//! and conflict resolution strategies.

use criterion::{black_box, criterion_group, criterion_main, BenchmarkId, Criterion};
use std::collections::HashMap;

use estnltk_core::{
    conflict_priority_resolver, keep_maximal_matches, keep_minimal_matches,
    Annotation, AnnotationValue, CommonConfig, ConflictStrategy, MatchSpan,
    TaggedSpan, TagResult, TaggerConfig,
};
use estnltk_taggers::{
    make_rule, make_phrase_rule, PhraseTagger, PhraseTaggerConfig, PhraseRule,
    RegexTagger, SpanRule, SpanTagger, SpanTaggerConfig, SubstringRule, SubstringTagger,
};

/// Representative Estonian text paragraph for benchmarks.
const ESTONIAN_BASE: &str = "\
Eesti Vabariik on demokraatlik riik Põhja-Euroopas. Eesti piirneb põhjas ja läänes \
Läänemeraga, lõunas Lätiga ja idas Venemaaga. Eesti pindala on 45 339 ruutkilomeetrit \
ja rahvaarv on ligikaudu 1,3 miljonit. Pealinn ja suurim linn on Tallinn. Eesti on \
Euroopa Liidu, NATO, Euroopa Nõukogu, OECD ja paljude teiste rahvusvaheliste \
organisatsioonide liige. Riigi ametlik keel on eesti keel, mis kuulub soome-ugri \
keelkonda. Eesti majandus on kõrgelt arenenud ja riik on üks maailma digitaalselt \
arenenumaid ühiskondi. E-residentsus, digitaalne allkirjastamine ja e-valitsemise \
lahendused on tuntud kogu maailmas. Eesti kultuur on rikas ja mitmekesine, hõlmates \
muusikat, kunsti, kirjandust ja traditsioone, mis ulatuvad sajandite taha. Laulupidu \
on üks olulisemaid kultuurisündmusi, mis koondab kokku tuhandeid lauljaid üle kogu \
riigi. Eesti loodus on mitmekesine, hõlmates metsi, rabasid, järvi ja pikka \
rannajoont. Riigis on üle 1500 saare, millest suurimad on Saaremaa ja Hiiumaa.";

/// Generate Estonian text of approximately the target byte size by repeating the base text.
fn generate_text(target_bytes: usize) -> String {
    let mut text = String::with_capacity(target_bytes + ESTONIAN_BASE.len());
    while text.len() < target_bytes {
        text.push_str(ESTONIAN_BASE);
        text.push(' ');
    }
    text
}

fn default_config(strategy: ConflictStrategy) -> TaggerConfig {
    TaggerConfig {
        common: CommonConfig {
            output_layer: "bench".to_string(),
            conflict_strategy: strategy,
            ..CommonConfig::default()
        },
        lowercase_text: false,
        overlapped: false,
        match_attribute: None,
    }
}

/// Common Estonian words/patterns for regex benchmarks.
const REGEX_PATTERNS: &[&str] = &[
    "Eesti",
    "[Rr]iik",
    "\\d+",
    "[A-ZÕÄÖÜ][a-zõäöü]+",
    "on",
    "[a-zõäöü]+mine",
    "ja",
    "[Pp]ealinn",
    "[Ll]inn",
    "[Kk]ultuur[a-zõäöü]*",
    "[Mm]ajandus[a-zõäöü]*",
    "[Dd]igitaal[a-zõäöü]*",
    "[Ee]uroopa",
    "[Ll]oodus[a-zõäöü]*",
    "[Ss]aar[a-zõäöü]*",
    "[Mm]ets[a-zõäöü]*",
    "[Rr]aba[a-zõäöü]*",
    "[Jj]ärv[a-zõäöü]*",
    "[Kk]eel[a-zõäöü]*",
    "[Mm]uusika[a-zõäöü]*",
    "[Kk]unst[a-zõäöü]*",
    "[Kk]irjandus[a-zõäöü]*",
    "[Tt]raditsioon[a-zõäöü]*",
    "[Ll]aulupidu[a-zõäöü]*",
    "[Oo]rganisatsioon[a-zõäöü]*",
    "[Rr]ahvusvaheli[a-zõäöü]*",
    "[Ll]ähene[a-zõäöü]*",
    "[Aa]rene[a-zõäöü]*",
    "[Uu]latu[a-zõäöü]*",
    "[Kk]oonda[a-zõäöü]*",
    "[Vv]abariik",
    "[Dd]emokraat[a-zõäöü]*",
    "[Pp]indala",
    "[Rr]ahvaarv",
    "[Mm]iljon[a-zõäöü]*",
    "[Aa]metlik",
    "[Ää]hiskond[a-zõäöü]*",
    "[Aa]llkirjasta[a-zõäöü]*",
    "[Vv]alitsemi[a-zõäöü]*",
    "[Mm]aailm[a-zõäöü]*",
    "[Mm]itmekesi[a-zõäöü]*",
    "[Ss]ajand[a-zõäöü]*",
    "[Rr]annajoon[a-zõäöü]*",
    "[Hh]iiumaa",
    "[Ss]aaremaa",
    "[Tt]allinn",
    "[Ll]ätiga",
    "[Vv]enemaaga",
    "[Ll]äänemere[a-zõäöü]*",
    "[Pp]õhja",
];

/// Common Estonian words for substring benchmarks.
const SUBSTRING_PATTERNS: &[&str] = &[
    "Eesti", "riik", "on", "ja", "linn", "keel", "mets", "saar", "järv", "raba",
    "Tallinn", "Saaremaa", "Hiiumaa", "Euroopa", "NATO", "kultuur", "muusika",
    "kunst", "kirjandus", "traditsioon", "laulupidu", "organisatsioon", "majandus",
    "digitaalne", "loodus", "pealinn", "vabariik", "demokraatlik", "pindala",
    "rahvaarv", "miljon", "ametlik", "ühiskond", "allkirjastamine", "valitsemine",
    "maailm", "mitmekesine", "sajand", "rannajoon", "Lätiga", "Venemaaga",
    "Läänemeri", "Põhja", "arenenud", "liige", "kuulub", "soome-ugri",
    "e-residentsus", "lahendused", "tuntud",
];

/// Build a RegexTagger from a slice of pattern strings.
fn build_regex_tagger(patterns: &[&str], config: TaggerConfig) -> RegexTagger {
    let rules = patterns
        .iter()
        .map(|p| make_rule(p, HashMap::new(), 0, 0).unwrap())
        .collect();
    RegexTagger::new(rules, config).unwrap()
}

// --- RegexTagger: varying text size ---

fn bench_regex_text_size(c: &mut Criterion) {
    let mut group = c.benchmark_group("regex_tagger/text_size");
    let patterns = &REGEX_PATTERNS[..10];

    for &size in &[1_000, 10_000, 100_000] {
        let text = generate_text(size);
        let label = format!("{}KB", size / 1000);
        let tagger = build_regex_tagger(patterns, default_config(ConflictStrategy::KeepMaximal));

        group.bench_with_input(BenchmarkId::new("tag", &label), &text, |b, text| {
            b.iter(|| {
                let result = tagger.tag(black_box(text));
                black_box(&result);
            });
        });
    }
    group.finish();
}

// --- RegexTagger: varying pattern count ---

fn bench_regex_pattern_count(c: &mut Criterion) {
    let mut group = c.benchmark_group("regex_tagger/pattern_count");
    let text = generate_text(10_000);

    for &count in &[5, 10, 25, 50] {
        let tagger = build_regex_tagger(
            &REGEX_PATTERNS[..count],
            default_config(ConflictStrategy::KeepMaximal),
        );

        group.bench_with_input(
            BenchmarkId::new("tag", format!("{}_patterns", count)),
            &text,
            |b, text| {
                b.iter(|| {
                    let result = tagger.tag(black_box(text));
                    black_box(&result);
                });
            },
        );
    }
    group.finish();
}

// --- SubstringTagger: varying text size ---

fn bench_substring_text_size(c: &mut Criterion) {
    let mut group = c.benchmark_group("substring_tagger/text_size");

    for &size in &[1_000, 10_000, 100_000] {
        let text = generate_text(size);
        let label = format!("{}KB", size / 1000);

        let rules: Vec<_> = SUBSTRING_PATTERNS[..10]
            .iter()
            .map(|p| SubstringRule::new(p, HashMap::new(), 0, 0))
            .collect();
        let tagger =
            SubstringTagger::new(rules, "", default_config(ConflictStrategy::KeepMaximal)).unwrap();

        group.bench_with_input(BenchmarkId::new("tag", &label), &text, |b, text| {
            b.iter(|| {
                let result = tagger.tag(black_box(text));
                black_box(&result);
            });
        });
    }
    group.finish();
}

// --- SubstringTagger: varying pattern count ---

fn bench_substring_pattern_count(c: &mut Criterion) {
    let mut group = c.benchmark_group("substring_tagger/pattern_count");
    let text = generate_text(10_000);

    for &count in &[5, 10, 25, 50] {
        let rules: Vec<_> = SUBSTRING_PATTERNS[..count]
            .iter()
            .map(|p| SubstringRule::new(p, HashMap::new(), 0, 0))
            .collect();
        let tagger =
            SubstringTagger::new(rules, "", default_config(ConflictStrategy::KeepMaximal)).unwrap();

        group.bench_with_input(
            BenchmarkId::new("tag", format!("{}_patterns", count)),
            &text,
            |b, text| {
                b.iter(|| {
                    let result = tagger.tag(black_box(text));
                    black_box(&result);
                });
            },
        );
    }
    group.finish();
}

// --- Conflict resolution: isolated benchmarks ---

fn generate_match_entries(n: usize) -> Vec<(MatchSpan, usize)> {
    (0..n)
        .map(|i| (MatchSpan::new(i * 3, i * 3 + 5), i % 10))
        .collect()
}

fn bench_conflict_resolution(c: &mut Criterion) {
    let mut group = c.benchmark_group("conflict_resolution");

    for &n in &[100, 1_000, 5_000] {
        let entries = generate_match_entries(n);
        let label = format!("{}_spans", n);

        group.bench_with_input(
            BenchmarkId::new("keep_maximal", &label),
            &entries,
            |b, entries| {
                b.iter(|| {
                    let result = keep_maximal_matches(black_box(entries));
                    black_box(&result);
                });
            },
        );

        group.bench_with_input(
            BenchmarkId::new("keep_minimal", &label),
            &entries,
            |b, entries| {
                b.iter(|| {
                    let result = keep_minimal_matches(black_box(entries));
                    black_box(&result);
                });
            },
        );

        let groups: Vec<i32> = entries.iter().map(|(_, ri)| *ri as i32).collect();
        let priorities: Vec<i32> = entries
            .iter()
            .enumerate()
            .map(|(i, _)| (i % 3) as i32)
            .collect();

        group.bench_with_input(
            BenchmarkId::new("priority_resolver", &label),
            &entries,
            |b, entries| {
                b.iter(|| {
                    let result =
                        conflict_priority_resolver(black_box(entries), &groups, &priorities);
                    black_box(&result);
                });
            },
        );
    }
    group.finish();
}

// --- Conflict strategy comparison on real tagger output ---

fn bench_regex_conflict_strategies(c: &mut Criterion) {
    let mut group = c.benchmark_group("regex_tagger/conflict_strategy");
    let text = generate_text(10_000);

    for &strategy in &[
        ConflictStrategy::KeepAll,
        ConflictStrategy::KeepMaximal,
        ConflictStrategy::KeepMinimal,
    ] {
        let name = match strategy {
            ConflictStrategy::KeepAll => "KEEP_ALL",
            ConflictStrategy::KeepMaximal => "KEEP_MAXIMAL",
            ConflictStrategy::KeepMinimal => "KEEP_MINIMAL",
            _ => unreachable!(),
        };

        let tagger = build_regex_tagger(&REGEX_PATTERNS[..10], default_config(strategy));

        group.bench_with_input(BenchmarkId::new("tag", name), &text, |b, text| {
            b.iter(|| {
                let result = tagger.tag(black_box(text));
                black_box(&result);
            });
        });
    }
    group.finish();
}

// --- Lowercase text overhead ---

fn bench_lowercase_overhead(c: &mut Criterion) {
    let mut group = c.benchmark_group("regex_tagger/lowercase");
    let text = generate_text(10_000);

    for &lowercase in &[false, true] {
        let mut cfg = default_config(ConflictStrategy::KeepMaximal);
        cfg.lowercase_text = lowercase;
        let tagger = build_regex_tagger(&REGEX_PATTERNS[..10], cfg);
        let label = if lowercase {
            "lowercase=true"
        } else {
            "lowercase=false"
        };

        group.bench_with_input(BenchmarkId::new("tag", label), &text, |b, text| {
            b.iter(|| {
                let result = tagger.tag(black_box(text));
                black_box(&result);
            });
        });
    }
    group.finish();
}

// --- SpanTagger benchmarks ---

/// Estonian lemmas for SpanTagger rules — maps lemma patterns to labels.
const SPAN_LEMMAS: &[(&str, &str)] = &[
    ("eesti", "LOC"), ("riik", "CONCEPT"), ("linn", "LOC"), ("keel", "LANG"),
    ("mets", "NATURE"), ("saar", "NATURE"), ("järv", "NATURE"), ("raba", "NATURE"),
    ("tallinn", "LOC"), ("saaremaa", "LOC"), ("hiiumaa", "LOC"), ("euroopa", "LOC"),
    ("kultuur", "CONCEPT"), ("muusika", "ART"), ("kunst", "ART"), ("kirjandus", "ART"),
    ("traditsioon", "CONCEPT"), ("laulupidu", "EVENT"), ("organisatsioon", "ORG"),
    ("majandus", "CONCEPT"), ("digitaalne", "TECH"), ("loodus", "NATURE"),
    ("pealinn", "LOC"), ("vabariik", "POLITY"), ("demokraatlik", "POLITY"),
    ("maailm", "LOC"), ("nato", "ORG"), ("oecd", "ORG"), ("liit", "ORG"),
    ("rahvaarv", "CONCEPT"),
];

/// Build a synthetic TagResult simulating morphological analysis output.
/// Each "word" gets a span and an annotation with a `lemma` attribute.
fn build_span_tagger_input(n_spans: usize) -> TagResult {
    let lemmas: Vec<&str> = SPAN_LEMMAS.iter().map(|(l, _)| *l).collect();
    let mut spans = Vec::with_capacity(n_spans);
    let mut offset = 0;
    for i in 0..n_spans {
        let lemma = lemmas[i % lemmas.len()];
        let end = offset + lemma.len();
        spans.push(TaggedSpan {
            span: MatchSpan::new(offset, end),
            annotations: vec![Annotation::from(HashMap::from([(
                "lemma".to_string(),
                AnnotationValue::Str(lemma.to_string()),
            )]))],
        });
        offset = end + 1; // +1 for space
    }
    TagResult {
        name: "morph_analysis".to_string(),
        attributes: vec!["lemma".to_string()],
        ambiguous: true,
        spans,
    }
}

fn build_span_tagger(n_rules: usize) -> SpanTagger {
    let rules: Vec<SpanRule> = SPAN_LEMMAS[..n_rules]
        .iter()
        .map(|(pattern, label)| {
            SpanRule::new(
                pattern,
                HashMap::from([("type".to_string(), AnnotationValue::Str(label.to_string()))]),
                0,
                0,
            )
        })
        .collect();
    let config = SpanTaggerConfig {
        common: CommonConfig {
            output_layer: "tagged".to_string(),
            output_attributes: vec!["type".to_string()],
            conflict_strategy: ConflictStrategy::KeepAll,
            ..CommonConfig::default()
        },
        input_attribute: "lemma".to_string(),
        ignore_case: false,
    };
    SpanTagger::new(rules, config).unwrap()
}

fn bench_span_tagger_input_size(c: &mut Criterion) {
    let mut group = c.benchmark_group("span_tagger/input_size");
    let tagger = build_span_tagger(10);

    for &n_spans in &[100, 1_000, 10_000] {
        let input = build_span_tagger_input(n_spans);
        let label = format!("{}_spans", n_spans);

        group.bench_with_input(BenchmarkId::new("tag", &label), &input, |b, input| {
            b.iter(|| {
                let result = tagger.tag(black_box(input));
                black_box(&result);
            });
        });
    }
    group.finish();
}

fn bench_span_tagger_rule_count(c: &mut Criterion) {
    let mut group = c.benchmark_group("span_tagger/rule_count");
    let input = build_span_tagger_input(1_000);

    for &n_rules in &[5, 10, 20, 30] {
        let tagger = build_span_tagger(n_rules);
        let label = format!("{}_rules", n_rules);

        group.bench_with_input(BenchmarkId::new("tag", &label), &input, |b, input| {
            b.iter(|| {
                let result = tagger.tag(black_box(input));
                black_box(&result);
            });
        });
    }
    group.finish();
}

fn bench_span_tagger_ignore_case(c: &mut Criterion) {
    let mut group = c.benchmark_group("span_tagger/ignore_case");
    let input = build_span_tagger_input(1_000);

    for &ignore_case in &[false, true] {
        let rules: Vec<SpanRule> = SPAN_LEMMAS[..10]
            .iter()
            .map(|(pattern, label)| {
                SpanRule::new(
                    pattern,
                    HashMap::from([("type".to_string(), AnnotationValue::Str(label.to_string()))]),
                    0,
                    0,
                )
            })
            .collect();
        let config = SpanTaggerConfig {
            common: CommonConfig {
                output_layer: "tagged".to_string(),
                output_attributes: vec!["type".to_string()],
                conflict_strategy: ConflictStrategy::KeepAll,
                ..CommonConfig::default()
            },
            input_attribute: "lemma".to_string(),
            ignore_case,
        };
        let tagger = SpanTagger::new(rules, config).unwrap();
        let label = if ignore_case { "ignore_case=true" } else { "ignore_case=false" };

        group.bench_with_input(BenchmarkId::new("tag", label), &input, |b, input| {
            b.iter(|| {
                let result = tagger.tag(black_box(input));
                black_box(&result);
            });
        });
    }
    group.finish();
}

// --- PhraseTagger benchmarks ---

/// Estonian phrase patterns (tuples) for PhraseTagger rules.
const PHRASE_PATTERNS: &[&[&str]] = &[
    &["euroopa", "liit"],
    &["eesti", "vabariik"],
    &["euroopa", "nõukogu"],
    &["soome", "ugri"],
    &["eesti", "keel"],
    &["eesti", "kultuur"],
    &["eesti", "loodus"],
    &["eesti", "majandus"],
    &["new", "york", "city"],
    &["põhja", "euroopa"],
    &["lääne", "meri"],
    &["rahvusvaheline", "organisatsioon"],
    &["digitaalne", "allkirjastamine"],
    &["eesti", "vabariik", "valitsus"],
    &["soome", "ugri", "keelkond"],
];

/// Build a synthetic TagResult for PhraseTagger input.
/// Creates `n_spans` spans cycling through Estonian lemmas that include phrase components.
fn build_phrase_tagger_input(n_spans: usize) -> TagResult {
    let words = [
        "eesti", "vabariik", "on", "euroopa", "liit", "ja", "euroopa", "nõukogu",
        "liige", "eesti", "keel", "kuulub", "soome", "ugri", "keelkonda",
        "eesti", "kultuur", "on", "rikas", "digitaalne", "allkirjastamine",
    ];
    let mut spans = Vec::with_capacity(n_spans);
    let mut offset = 0;
    for i in 0..n_spans {
        let word = words[i % words.len()];
        let end = offset + word.len();
        spans.push(TaggedSpan {
            span: MatchSpan::new(offset, end),
            annotations: vec![Annotation::from(HashMap::from([(
                "lemma".to_string(),
                AnnotationValue::Str(word.to_string()),
            )]))],
        });
        offset = end + 1;
    }
    TagResult {
        name: "morph_analysis".to_string(),
        attributes: vec!["lemma".to_string()],
        ambiguous: true,
        spans,
    }
}

fn build_phrase_tagger(n_rules: usize) -> PhraseTagger {
    let rules: Vec<PhraseRule> = PHRASE_PATTERNS[..n_rules]
        .iter()
        .map(|pattern| {
            make_phrase_rule(
                pattern.iter().map(|s| s.to_string()).collect(),
                HashMap::from([("value".to_string(), AnnotationValue::Str("ENTITY".to_string()))]),
                0,
                0,
            )
        })
        .collect();
    let config = PhraseTaggerConfig {
        common: CommonConfig {
            output_layer: "phrases".to_string(),
            output_attributes: vec!["value".to_string()],
            conflict_strategy: ConflictStrategy::KeepAll,
            ..CommonConfig::default()
        },
        input_attribute: "lemma".to_string(),
        ignore_case: false,
        phrase_attribute: Some("phrase".to_string()),
    };
    PhraseTagger::new(rules, config).unwrap()
}

fn bench_phrase_tagger_input_size(c: &mut Criterion) {
    let mut group = c.benchmark_group("phrase_tagger/input_size");
    let tagger = build_phrase_tagger(8);

    for &n_spans in &[100, 1_000, 10_000] {
        let input = build_phrase_tagger_input(n_spans);
        let label = format!("{}_spans", n_spans);

        group.bench_with_input(BenchmarkId::new("tag", &label), &input, |b, input| {
            b.iter(|| {
                let result = tagger.tag(black_box(input));
                black_box(&result);
            });
        });
    }
    group.finish();
}

fn bench_phrase_tagger_rule_count(c: &mut Criterion) {
    let mut group = c.benchmark_group("phrase_tagger/rule_count");
    let input = build_phrase_tagger_input(1_000);

    for &n_rules in &[3, 8, 15] {
        let tagger = build_phrase_tagger(n_rules);
        let label = format!("{}_rules", n_rules);

        group.bench_with_input(BenchmarkId::new("tag", &label), &input, |b, input| {
            b.iter(|| {
                let result = tagger.tag(black_box(input));
                black_box(&result);
            });
        });
    }
    group.finish();
}

fn bench_phrase_tagger_ignore_case(c: &mut Criterion) {
    let mut group = c.benchmark_group("phrase_tagger/ignore_case");
    let input = build_phrase_tagger_input(1_000);

    for &ignore_case in &[false, true] {
        let rules: Vec<PhraseRule> = PHRASE_PATTERNS[..8]
            .iter()
            .map(|pattern| {
                make_phrase_rule(
                    pattern.iter().map(|s| s.to_string()).collect(),
                    HashMap::from([("value".to_string(), AnnotationValue::Str("ENTITY".to_string()))]),
                    0,
                    0,
                )
            })
            .collect();
        let config = PhraseTaggerConfig {
            common: CommonConfig {
                output_layer: "phrases".to_string(),
                output_attributes: vec!["value".to_string()],
                conflict_strategy: ConflictStrategy::KeepAll,
                ..CommonConfig::default()
            },
            input_attribute: "lemma".to_string(),
            ignore_case,
            phrase_attribute: Some("phrase".to_string()),
        };
        let tagger = PhraseTagger::new(rules, config).unwrap();
        let label = if ignore_case { "ignore_case=true" } else { "ignore_case=false" };

        group.bench_with_input(BenchmarkId::new("tag", label), &input, |b, input| {
            b.iter(|| {
                let result = tagger.tag(black_box(input));
                black_box(&result);
            });
        });
    }
    group.finish();
}

criterion_group!(
    tagger_bench_group,
    bench_regex_text_size,
    bench_regex_pattern_count,
    bench_substring_text_size,
    bench_substring_pattern_count,
    bench_conflict_resolution,
    bench_regex_conflict_strategies,
    bench_lowercase_overhead,
    bench_span_tagger_input_size,
    bench_span_tagger_rule_count,
    bench_span_tagger_ignore_case,
    bench_phrase_tagger_input_size,
    bench_phrase_tagger_rule_count,
    bench_phrase_tagger_ignore_case,
);

criterion_main!(tagger_bench_group);
