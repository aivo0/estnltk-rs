/// Sentence merge pattern and postcorrection tests.
///
/// Expected values verified against Python EstNLTK v1.7.x.
/// Tests each category of sentence merge rule and each postcorrection step.

mod common;

use estnltk_segmentation::SegmentationPipeline;

fn pipeline() -> SegmentationPipeline {
    SegmentationPipeline::estonian()
}

// ===== MERGE RULES BY CATEGORY =====

#[test]
fn test_merge_numeric_range() {
    let text = "Tartu Muinsuskaitseп\u{00E4}evad toimusid 1988. a 14. - 17. aprillil. Tegelikult oli soov need teha n\u{00E4}dal hiljem.";
    let r = pipeline().segment(text);
    assert_eq!(
        common::sentence_texts(text, &r),
        vec![
            "Tartu Muinsuskaitseп\u{00E4}evad toimusid 1988. a 14. - 17. aprillil.",
            "Tegelikult oli soov need teha n\u{00E4}dal hiljem.",
        ]
    );
}

#[test]
fn test_merge_year_abbreviation() {
    let text = "Luunja sai valla\u{00F5}igused 1991.a. kevadel.";
    let r = pipeline().segment(text);
    assert_eq!(
        common::sentence_texts(text, &r),
        vec!["Luunja sai valla\u{00F5}igused 1991.a. kevadel."]
    );
}

#[test]
fn test_merge_date_time() {
    let text = "Gert 02.03.2009. 14:40 Tahaks kindlalt sinna kooli:P";
    let r = pipeline().segment(text);
    assert_eq!(
        common::sentence_texts(text, &r),
        vec!["Gert 02.03.2009. 14:40 Tahaks kindlalt sinna kooli:P"]
    );
}

#[test]
fn test_merge_kell_time() {
    let text = "Kell 15 . 50 tuli elekter Tallinna tagasi .";
    let r = pipeline().segment(text);
    assert_eq!(
        common::sentence_texts(text, &r),
        vec!["Kell 15 . 50 tuli elekter Tallinna tagasi ."]
    );
}

#[test]
fn test_merge_year_aastal() {
    let text = "BRK-de traditsioon sai alguse 1964 . aastal Saksamaal Heidelbergis.";
    let r = pipeline().segment(text);
    assert_eq!(
        common::sentence_texts(text, &r),
        vec!["BRK-de traditsioon sai alguse 1964 . aastal Saksamaal Heidelbergis."]
    );
}

#[test]
fn test_merge_century() {
    let text = "Kui sealt alla sammusin siis leitsin 15. saj. p\u{00E4}rit surnuaia .\nV\u{00F5}i oli isegi pikem aeg , 19. saj. l\u{00F5}pust , kusagilt lugesin .";
    let r = pipeline().segment(text);
    assert_eq!(
        common::sentence_texts(text, &r),
        vec![
            "Kui sealt alla sammusin siis leitsin 15. saj. p\u{00E4}rit surnuaia .",
            "V\u{00F5}i oli isegi pikem aeg , 19. saj. l\u{00F5}pust , kusagilt lugesin .",
        ]
    );
}

#[test]
fn test_merge_bce() {
    let text = "Aastaks 325 p.Kr. olid erinevad kristlikud sektid omavahel t\u{00FC}lli l\u{00E4}inud.";
    let r = pipeline().segment(text);
    assert_eq!(
        common::sentence_texts(text, &r),
        vec!["Aastaks 325 p.Kr. olid erinevad kristlikud sektid omavahel t\u{00FC}lli l\u{00E4}inud."]
    );
}

#[test]
fn test_merge_date_month() {
    let text = "Aga selgust ei pruugi enne 15 . augustit tulla .";
    let r = pipeline().segment(text);
    assert_eq!(
        common::sentence_texts(text, &r),
        vec!["Aga selgust ei pruugi enne 15 . augustit tulla ."]
    );
}

#[test]
fn test_merge_roman_numeral() {
    let text = "Rooma ja Kartaago vahel III. - II. sajandil enne meie ajastut Vahemeremaade valitsemise p\u{00E4}rast toimunud s\u{00F5}jad.";
    let r = pipeline().segment(text);
    assert_eq!(
        common::sentence_texts(text, &r),
        vec!["Rooma ja Kartaago vahel III. - II. sajandil enne meie ajastut Vahemeremaade valitsemise p\u{00E4}rast toimunud s\u{00F5}jad."]
    );
}

#[test]
fn test_merge_ordinal() {
    let text = "6 . augustil m\u{00E4}ngitakse ette s\u{00FC}gisringi 4 . vooru kohtumine.";
    let r = pipeline().segment(text);
    assert_eq!(
        common::sentence_texts(text, &r),
        vec!["6 . augustil m\u{00E4}ngitakse ette s\u{00FC}gisringi 4 . vooru kohtumine."]
    );
}

#[test]
fn test_merge_monetary() {
    let text = "Siiski tahavad erinevad tegelejad asja eest nii 1500. - kuni 3000. - krooni saada.";
    let r = pipeline().segment(text);
    assert_eq!(
        common::sentence_texts(text, &r),
        vec!["Siiski tahavad erinevad tegelejad asja eest nii 1500. - kuni 3000. - krooni saada."]
    );
}

#[test]
fn test_merge_name_initial() {
    let text = "A. H. Tammsaare 1935. aastal: 1,0 m / s = 3,67 km/h.";
    let r = pipeline().segment(text);
    assert_eq!(
        common::sentence_texts(text, &r),
        vec!["A. H. Tammsaare 1935. aastal: 1,0 m / s = 3,67 km/h."]
    );
}

// ===== POSTCORRECTION TESTS =====

#[test]
fn test_postcorrect_parentheses_merge() {
    let text = "Lugesime Menippose (III saj. e.m.a.) satiiri...";
    let r = pipeline().segment(text);
    assert_eq!(
        common::sentence_texts(text, &r),
        vec!["Lugesime Menippose (III saj. e.m.a.) satiiri..."]
    );
}

#[test]
fn test_postcorrect_parentheses_date() {
    let text = "Murelik lugeja kurdab ( EPL 31.03. ) , et valla on p\u{00E4}\u{00E4}senud kolmas maailmas\u{00F5}da .";
    let r = pipeline().segment(text);
    assert_eq!(
        common::sentence_texts(text, &r),
        vec!["Murelik lugeja kurdab ( EPL 31.03. ) , et valla on p\u{00E4}\u{00E4}senud kolmas maailmas\u{00F5}da ."]
    );
}

#[test]
fn test_postcorrect_double_newline_split() {
    let text = "Esimene lause.\n\nTeine l\u{00F5}ik lause.";
    let r = pipeline().segment(text);
    assert_eq!(
        common::sentence_texts(text, &r),
        vec!["Esimene lause.", "Teine l\u{00F5}ik lause."]
    );
}

#[test]
fn test_postcorrect_compound_token_no_split() {
    // The date compound token "02.02.2010" should not cause a sentence split
    // at the internal periods
    let text = "Kuup\u{00E4}ev on 02.02.2010 ja rohkem pole.";
    let r = pipeline().segment(text);
    assert_eq!(r.sentences.len(), 1);
    assert_eq!(
        common::sentence_texts(text, &r),
        vec!["Kuup\u{00E4}ev on 02.02.2010 ja rohkem pole."]
    );
}
