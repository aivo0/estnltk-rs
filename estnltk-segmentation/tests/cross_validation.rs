/// Cross-implementation validation tests.
///
/// Expected values verified against Python EstNLTK v1.7.x.
/// These tests ensure the Rust segmentation pipeline produces identical
/// word/sentence/paragraph boundaries as the Python implementation.

mod common;

use estnltk_segmentation::SegmentationPipeline;

fn pipeline() -> SegmentationPipeline {
    SegmentationPipeline::estonian()
}

#[test]
fn test_canonical_pipeline_1() {
    let text = "Aadressilt bla@bla.ee tuli 10 000 kirja. Kirjad, st. spamm saabus 10 tunni jooksul.\n\nA. H. Tammsaare 1935. aastal: 1,0 m / s = 3,67 km/h.";
    let p = pipeline();
    let r = p.segment(text);

    let expected_tokens = vec![
        "Aadressilt", "bla", "@", "bla", ".", "ee", "tuli", "10", "000", "kirja", ".",
        "Kirjad", ",", "st", ".", "spamm", "saabus", "10", "tunni", "jooksul", ".",
        "A", ".", "H", ".", "Tammsaare", "1935", ".", "aastal", ":", "1", ",", "0",
        "m", "/", "s", "=", "3", ",", "67", "km", "/", "h", ".",
    ];
    assert_eq!(common::token_texts(text, &r), expected_tokens);

    let expected_words = vec![
        "Aadressilt", "bla@bla.ee", "tuli", "10 000", "kirja", ".", "Kirjad", ",",
        "st.", "spamm", "saabus", "10", "tunni", "jooksul", ".",
        "A. H. Tammsaare", "1935.", "aastal", ":", "1,0", "m / s", "=", "3,67", "km/h", ".",
    ];
    assert_eq!(common::word_texts(text, &r), expected_words);

    let expected_sentences = vec![
        "Aadressilt bla@bla.ee tuli 10 000 kirja.",
        "Kirjad, st. spamm saabus 10 tunni jooksul.",
        "A. H. Tammsaare 1935. aastal: 1,0 m / s = 3,67 km/h.",
    ];
    assert_eq!(common::sentence_texts(text, &r), expected_sentences);

    assert_eq!(r.paragraphs.len(), 2);
}

#[test]
fn test_canonical_pipeline_2() {
    let text = "Aadressilt bla@bla.ee tuli 10 000 kirja, st. spammi aadressile foo@foo.ee 10 tunni jooksul 2017. aastal. \nA. H. Tammsaare: 1,0 m / s = 3, 67 km/h.";
    let p = pipeline();
    let r = p.segment(text);

    let expected_words = vec![
        "Aadressilt", "bla@bla.ee", "tuli", "10 000", "kirja", ",", "st.", "spammi",
        "aadressile", "foo@foo.ee", "10", "tunni", "jooksul", "2017.", "aastal", ".",
        "A. H. Tammsaare", ":", "1,0", "m / s", "=", "3", ",", "67", "km/h", ".",
    ];
    assert_eq!(common::word_texts(text, &r), expected_words);
}

#[test]
fn test_two_sentences() {
    let text = "Tere maailm. Kuidas l\u{00E4}heb?";
    let p = pipeline();
    let r = p.segment(text);

    assert_eq!(
        common::word_texts(text, &r),
        vec!["Tere", "maailm", ".", "Kuidas", "l\u{00E4}heb", "?"]
    );
    assert_eq!(
        common::sentence_texts(text, &r),
        vec!["Tere maailm.", "Kuidas l\u{00E4}heb?"]
    );
    assert_eq!(r.paragraphs.len(), 1);
}

#[test]
fn test_estonian_with_compounds() {
    let text = "Eesti Vabariik on riik P\u{00F5}hja-Euroopas. Pealinn on Tallinn.";
    let p = pipeline();
    let r = p.segment(text);

    assert_eq!(
        common::word_texts(text, &r),
        vec!["Eesti", "Vabariik", "on", "riik", "P\u{00F5}hja-Euroopas", ".", "Pealinn", "on", "Tallinn", "."]
    );
    assert_eq!(
        common::sentence_texts(text, &r),
        vec!["Eesti Vabariik on riik P\u{00F5}hja-Euroopas.", "Pealinn on Tallinn."]
    );
    assert_eq!(r.paragraphs.len(), 1);
}

#[test]
fn test_date_compound() {
    let text = "Kuup\u{00E4}ev on 02.02.2010 ja see on hea.";
    let p = pipeline();
    let r = p.segment(text);

    assert_eq!(
        common::word_texts(text, &r),
        vec!["Kuup\u{00E4}ev", "on", "02.02.2010", "ja", "see", "on", "hea", "."]
    );
    assert_eq!(
        common::sentence_texts(text, &r),
        vec!["Kuup\u{00E4}ev on 02.02.2010 ja see on hea."]
    );
    assert_eq!(r.paragraphs.len(), 1);
}

#[test]
fn test_multiple_paragraphs() {
    let text = "Esimene lause.\n\nTeine l\u{00F5}ik.\n\nKolmas.";
    let p = pipeline();
    let r = p.segment(text);

    assert_eq!(
        common::word_texts(text, &r),
        vec!["Esimene", "lause", ".", "Teine", "l\u{00F5}ik", ".", "Kolmas", "."]
    );
    assert_eq!(
        common::sentence_texts(text, &r),
        vec!["Esimene lause.", "Teine l\u{00F5}ik.", "Kolmas."]
    );
    assert_eq!(r.paragraphs.len(), 3);
}

#[test]
fn test_empty_input() {
    let p = pipeline();
    let r = p.segment("");
    assert!(r.tokens.is_empty());
    assert!(r.words.is_empty());
    assert!(r.sentences.is_empty());
    assert!(r.paragraphs.is_empty());
}

#[test]
fn test_single_word() {
    let text = "Tere";
    let p = pipeline();
    let r = p.segment(text);

    assert_eq!(common::word_texts(text, &r), vec!["Tere"]);
    assert_eq!(common::sentence_texts(text, &r), vec!["Tere"]);
    assert_eq!(r.paragraphs.len(), 1);
}
