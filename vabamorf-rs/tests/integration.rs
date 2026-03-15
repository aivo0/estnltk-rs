use std::path::Path;
use vabamorf_rs::{syllabify, Vabamorf};

fn dct_dir() -> &'static Path {
    Path::new("../../estnltk-src/estnltk/estnltk/vabamorf/dct/2020-01-22_sp")
}

fn make_vm() -> Vabamorf {
    Vabamorf::from_dct_dir(dct_dir()).expect("Failed to create Vabamorf instance")
}

#[test]
fn test_analyze_basic() {
    let mut vm = make_vm();
    let results = vm
        .analyze(&["tere", "maailm"], true, true, false, true, false)
        .unwrap();
    assert_eq!(results.len(), 2);
    assert_eq!(results[0].word, "tere");
    assert_eq!(results[1].word, "maailm");
    assert!(!results[0].analyses.is_empty());
    assert!(!results[1].analyses.is_empty());
}

#[test]
fn test_analyze_disambiguation() {
    let mut vm = make_vm();
    // "maja" with disambiguation should ideally return fewer analyses
    let with_disamb = vm
        .analyze(&["maja"], true, true, false, false, false)
        .unwrap();
    let without_disamb = vm
        .analyze(&["maja"], false, true, false, false, false)
        .unwrap();
    assert!(!with_disamb[0].analyses.is_empty());
    assert!(!without_disamb[0].analyses.is_empty());
    // Disambiguation should return <= analyses than without
    assert!(with_disamb[0].analyses.len() <= without_disamb[0].analyses.len());
}

#[test]
fn test_analyze_fields() {
    let mut vm = make_vm();
    let results = vm
        .analyze(&["maja"], false, true, false, false, false)
        .unwrap();
    let a = &results[0].analyses[0];
    // Should have non-empty root and partofspeech
    assert!(!a.root.is_empty());
    assert!(!a.partofspeech.is_empty());
}

#[test]
fn test_analyze_empty_input() {
    let mut vm = make_vm();
    let results = vm
        .analyze(&[], true, true, false, true, false)
        .unwrap();
    assert!(results.is_empty());
}

#[test]
fn test_spellcheck_correct() {
    let mut vm = make_vm();
    let results = vm.spellcheck(&["tere"], true).unwrap();
    assert_eq!(results.len(), 1);
    assert!(results[0].correct, "\"tere\" should be spelled correctly");
}

#[test]
fn test_spellcheck_incorrect() {
    let mut vm = make_vm();
    let results = vm.spellcheck(&["terx"], true).unwrap();
    assert_eq!(results.len(), 1);
    assert!(
        !results[0].correct,
        "\"terx\" should be spelled incorrectly"
    );
    // Note: suggestions may or may not be returned depending on dictionary variant
}

#[test]
fn test_synthesize() {
    let mut vm = make_vm();
    let results = vm
        .synthesize("maja", "sg g", "S", "", true, false)
        .unwrap();
    assert!(!results.is_empty(), "Should synthesize at least one form");
    // Singular genitive of "maja" should be "maja"
    assert!(
        results.contains(&"maja".to_string()),
        "sg g of 'maja' should contain 'maja', got: {:?}",
        results
    );
}

#[test]
fn test_syllabify() {
    let result = syllabify("tere").unwrap();
    assert!(!result.is_empty(), "\"tere\" should have syllables");
    // "tere" has 2 syllables: "te" + "re"
    assert_eq!(result.len(), 2, "\"tere\" should have 2 syllables");
    assert_eq!(result[0].syllable, "te");
    assert_eq!(result[1].syllable, "re");
}

#[test]
fn test_invalid_dct_path() {
    let result = Vabamorf::new("/nonexistent/et.dct", "/nonexistent/et3.dct");
    assert!(result.is_err(), "Should fail with invalid dictionary paths");
}

#[test]
fn test_from_dct_dir_missing() {
    let result = Vabamorf::from_dct_dir(Path::new("/nonexistent"));
    assert!(result.is_err());
}
