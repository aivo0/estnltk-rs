use regex::Regex;

use crate::estonian::*;
use super::pattern_types::{CompoundTokenPattern, NormalizationAction, flatten_priority};

/// Build all 1st level (strict) patterns.
pub fn build_level1_patterns() -> Vec<CompoundTokenPattern> {
    let mut patterns = Vec::new();

    // === XML patterns ===
    patterns.push(CompoundTokenPattern {
        regex: Regex::new(r"(<[^<>]{1,25}?>)").unwrap(),
        pattern_type: "xml_tag".into(),
        group: 0,
        priority: flatten_priority(&[0, 0, 0, 1]),
        normalization: NormalizationAction::None,
        is_negative: false,
        left_strict: Option::None,
        right_strict: Option::None,
    });
    patterns.push(CompoundTokenPattern {
        regex: Regex::new(r#"(<[^<>]+?=[""][^<>]+?>)"#).unwrap(),
        pattern_type: "xml_tag".into(),
        group: 0,
        priority: flatten_priority(&[0, 0, 0, 2]),
        normalization: NormalizationAction::None,
        is_negative: false,
        left_strict: Option::None,
        right_strict: Option::None,
    });

    // === Email patterns ===
    patterns.push(CompoundTokenPattern {
        regex: Regex::new(&format!(
            r"([{ALPHANUM}_.+-]+(\(at\)|\[at\]|@)[{ALPHANUM}\-]+\.[{ALPHANUM}\-.]+)",
            ALPHANUM = ALPHANUM
        )).unwrap(),
        pattern_type: "email".into(),
        group: 1,
        priority: flatten_priority(&[0, 0, 1]),
        normalization: NormalizationAction::None,
        is_negative: false,
        left_strict: Option::None,
        right_strict: Option::None,
    });
    patterns.push(CompoundTokenPattern {
        regex: Regex::new(&format!(
            r"([{ALPHANUM}_+\-]+\s?\.\s?[{ALPHANUM}_+\-]+\s?(\[\s?\-at\-\s?\]|\(at\)|\[at\]|@)\s?[{ALPHANUM}\-]+\s?\.\s?[{ALPHANUM}_.+\-]+)",
            ALPHANUM = ALPHANUM
        )).unwrap(),
        pattern_type: "email".into(),
        group: 1,
        priority: flatten_priority(&[0, 0, 2]),
        normalization: NormalizationAction::StripWhitespaceGroup(1),
        is_negative: false,
        left_strict: Option::None,
        right_strict: Option::None,
    });

    // === WWW address patterns ===
    // http(s)://www.domain.ext
    patterns.push(CompoundTokenPattern {
        regex: Regex::new(&format!(
            r"(https?\s*:\s*(/+)\s*www[2-6]?\s*\.\s*[{ALPHANUM}_\-]+\s*\.\s*[{ALPHANUM}_.\-/]+(\?\S+|\#\S+)?)",
            ALPHANUM = ALPHANUM
        )).unwrap(),
        pattern_type: "www_address".into(),
        group: 1,
        priority: flatten_priority(&[0, 0, 3]),
        normalization: NormalizationAction::StripWhitespaceGroup(1),
        is_negative: false,
        left_strict: Option::None,
        right_strict: Option::None,
    });
    // http(s)://domain.ext
    patterns.push(CompoundTokenPattern {
        regex: Regex::new(&format!(
            r"(https?\s*:\s*(/+)\s*[{ALPHANUM}_\-]+\s*\.\s*[{ALPHANUM}_.\-/]+(\?\S+|\#\S+)?)",
            ALPHANUM = ALPHANUM
        )).unwrap(),
        pattern_type: "www_address".into(),
        group: 1,
        priority: flatten_priority(&[0, 0, 4]),
        normalization: NormalizationAction::StripWhitespaceGroup(1),
        is_negative: false,
        left_strict: Option::None,
        right_strict: Option::None,
    });
    // www.domain.ext
    patterns.push(CompoundTokenPattern {
        regex: Regex::new(&format!(
            r"(www[2-6]?\s*\.\s*[{ALPHANUM}_\-]+\s*\.\s*[{ALPHANUM}_.\-/]+(\?\S+|\#\S+)?)",
            ALPHANUM = ALPHANUM
        )).unwrap(),
        pattern_type: "www_address".into(),
        group: 1,
        priority: flatten_priority(&[0, 0, 5]),
        normalization: NormalizationAction::StripWhitespaceGroup(1),
        is_negative: false,
        left_strict: Option::None,
        right_strict: Option::None,
    });
    // Short web address: domain.tld
    patterns.push(CompoundTokenPattern {
        regex: Regex::new(&format!(
            r"(^|[^{ALPHANUM}])([{ALPHANUM}_\-.]+(\s\.\s|\.)(?:ee|org|edu|com|uk|ru|fi|lv|lt|eu|se|nl|de|dk))([^{ALPHANUM}]|$)",
            ALPHANUM = ALPHANUM
        )).unwrap(),
        pattern_type: "www_address_short".into(),
        group: 2,
        priority: flatten_priority(&[0, 0, 6]),
        normalization: NormalizationAction::StripWhitespaceGroup(2),
        is_negative: false,
        left_strict: Option::None,
        right_strict: Option::None,
    });

    // === Emoticon patterns ===
    // #1: :=)
    patterns.push(CompoundTokenPattern {
        regex: Regex::new(r"([;:][=\-]*[\)|\(ODP]+)").unwrap(),
        pattern_type: "emoticon".into(),
        group: 0,
        priority: flatten_priority(&[1, 0, 1]),
        normalization: NormalizationAction::StripWhitespace,
        is_negative: false,
        left_strict: Option::None,
        right_strict: Option::None,
    });
    // #2: :-S
    patterns.push(CompoundTokenPattern {
        regex: Regex::new(r"(\s)(:\-?[Ss]|:S:S:S)(\s)").unwrap(),
        pattern_type: "emoticon".into(),
        group: 2,
        priority: flatten_priority(&[1, 0, 2]),
        normalization: NormalizationAction::StripWhitespaceGroup(2),
        is_negative: false,
        left_strict: Option::None,
        right_strict: Option::None,
    });
    // #3: :-o
    patterns.push(CompoundTokenPattern {
        regex: Regex::new(r"(\s)([:;][\-']+[(\[/\*o9]+)(\s)").unwrap(),
        pattern_type: "emoticon".into(),
        group: 2,
        priority: flatten_priority(&[1, 0, 3]),
        normalization: NormalizationAction::StripWhitespaceGroup(2),
        is_negative: false,
        left_strict: Option::None,
        right_strict: Option::None,
    });
    // #4: :o )
    patterns.push(CompoundTokenPattern {
        regex: Regex::new(r"(\s)((=|:\-|[;:]o)\s\)(\s\))*)(\s)").unwrap(),
        pattern_type: "emoticon".into(),
        group: 2,
        priority: flatten_priority(&[1, 0, 4]),
        normalization: NormalizationAction::StripWhitespaceGroup(2),
        is_negative: false,
        left_strict: Option::None,
        right_strict: Option::None,
    });
    // #5: :// (but not after http/https - we handle lookbehind with post-check)
    patterns.push(CompoundTokenPattern {
        regex: Regex::new(r"(\s)([:;][\[/\]@o]+)(\s)").unwrap(),
        pattern_type: "emoticon".into(),
        group: 2,
        priority: flatten_priority(&[1, 0, 5]),
        normalization: NormalizationAction::StripWhitespaceGroup(2),
        is_negative: false,
        left_strict: Option::None,
        right_strict: Option::None,
    });
    // #6: : D
    patterns.push(CompoundTokenPattern {
        regex: Regex::new(r"(\s)([:;]\sD)(\s)").unwrap(),
        pattern_type: "emoticon".into(),
        group: 2,
        priority: flatten_priority(&[1, 0, 6]),
        normalization: NormalizationAction::StripWhitespaceGroup(2),
        is_negative: false,
        left_strict: Option::None,
        right_strict: Option::None,
    });
    // #7: :K
    patterns.push(CompoundTokenPattern {
        regex: Regex::new(r"(:[KL])(\s)").unwrap(),
        pattern_type: "emoticon".into(),
        group: 1,
        priority: flatten_priority(&[1, 0, 7]),
        normalization: NormalizationAction::StripWhitespaceGroup(1),
        is_negative: false,
        left_strict: Option::None,
        right_strict: Option::None,
    });

    // === Hashtag and username patterns ===
    patterns.push(CompoundTokenPattern {
        regex: Regex::new(&format!(
            r"(\#[{ALPHANUM}_]+)",
            ALPHANUM = ALPHANUM
        )).unwrap(),
        pattern_type: "hashtag".into(),
        group: 0,
        priority: flatten_priority(&[1, 0, 8]),
        normalization: NormalizationAction::StripWhitespace,
        is_negative: false,
        left_strict: Option::None,
        right_strict: Option::None,
    });
    patterns.push(CompoundTokenPattern {
        regex: Regex::new(&format!(
            r"(@[{ALPHANUM}_]+)",
            ALPHANUM = ALPHANUM
        )).unwrap(),
        pattern_type: "username_mention".into(),
        group: 0,
        priority: flatten_priority(&[1, 0, 9]),
        normalization: NormalizationAction::StripWhitespace,
        is_negative: false,
        left_strict: Option::None,
        right_strict: Option::None,
    });

    // === Number patterns ===
    // Date: dd.mm.yyyy
    patterns.push(CompoundTokenPattern {
        regex: Regex::new(
            r"(3[01]|[12][0-9]|0?[0-9])\s?\.\s?([012][0-9]|1[012])\s?\.\s?(1[7-9]\d\d|2[0-2]\d\d)a?"
        ).unwrap(),
        pattern_type: "numeric_date".into(),
        group: 0,
        priority: flatten_priority(&[2, 0, 1]),
        normalization: NormalizationAction::StripWhitespace,
        is_negative: false,
        left_strict: Option::None,
        right_strict: Option::None,
    });
    // Date: yyyy-mm-dd
    patterns.push(CompoundTokenPattern {
        regex: Regex::new(
            r"(1[7-9]\d\d|2[0-2]\d\d)-(0[1-9]|1[012])-(3[01]|[12][0-9]|0?[0-9])"
        ).unwrap(),
        pattern_type: "numeric_date".into(),
        group: 0,
        priority: flatten_priority(&[2, 0, 2]),
        normalization: NormalizationAction::StripWhitespace,
        is_negative: false,
        left_strict: Option::None,
        right_strict: Option::None,
    });
    // Date: dd/mm/yy
    patterns.push(CompoundTokenPattern {
        regex: Regex::new(
            r"(0[0-9]|[12][0-9]|3[01])/(0[1-9]|1[012])/(1[7-9]\d\d|2[0-2]\d\d|[7-9][0-9]|[0-3][0-9])"
        ).unwrap(),
        pattern_type: "numeric_date".into(),
        group: 0,
        priority: flatten_priority(&[2, 0, 3, 1]),
        normalization: NormalizationAction::StripWhitespace,
        is_negative: false,
        left_strict: Option::None,
        right_strict: Option::None,
    });
    // Date: dd. roman_mm yyyy
    patterns.push(CompoundTokenPattern {
        regex: Regex::new(
            r"(0[0-9]|[12][0-9]|3[01])\.\s+(I{1,3}|IV|V|VI{1,3}|I{1,2}X|X|XI{1,2})\s+(1[7-9]\d\d|2[0-2]\d\d)"
        ).unwrap(),
        pattern_type: "numeric_date".into(),
        group: 0,
        priority: flatten_priority(&[2, 0, 3, 2]),
        normalization: NormalizationAction::CollapseWhitespace,
        is_negative: false,
        left_strict: Option::None,
        right_strict: Option::None,
    });
    // Time: HH:mm(:ss)
    patterns.push(CompoundTokenPattern {
        regex: Regex::new(
            r"(0[0-9]|[12][0-9]|2[0123])\s?:\s?([0-5][0-9])(\s?:\s?([0-5][0-9]))?"
        ).unwrap(),
        pattern_type: "numeric_time".into(),
        group: 0,
        priority: flatten_priority(&[2, 0, 4]),
        normalization: NormalizationAction::StripWhitespace,
        is_negative: false,
        left_strict: Option::None,
        right_strict: Option::None,
    });

    // Generic numerics: 5 groups
    patterns.push(CompoundTokenPattern {
        regex: Regex::new(
            r"\d+[\ \.]+\d+[\ \.]+\d+[\ \.]+\d+[\ \.]+\d+( , \d+|,\d+)?"
        ).unwrap(),
        pattern_type: "numeric".into(),
        group: 0,
        priority: flatten_priority(&[2, 1, 0]),
        normalization: NormalizationAction::StripPeriods,
        is_negative: false,
        left_strict: Option::None,
        right_strict: Option::None,
    });
    // 4 groups
    patterns.push(CompoundTokenPattern {
        regex: Regex::new(
            r"\d+[\ \.]+\d+[\ \.]+\d+[\ \.]+\d+( , \d+|,\d+)?"
        ).unwrap(),
        pattern_type: "numeric".into(),
        group: 0,
        priority: flatten_priority(&[2, 1, 1]),
        normalization: NormalizationAction::StripPeriods,
        is_negative: false,
        left_strict: Option::None,
        right_strict: Option::None,
    });
    // 3 groups
    patterns.push(CompoundTokenPattern {
        regex: Regex::new(
            r"\d+[\ \.]+\d+[\ \.]+\d+( , \d+|,\d+)?"
        ).unwrap(),
        pattern_type: "numeric".into(),
        group: 0,
        priority: flatten_priority(&[2, 1, 2]),
        normalization: NormalizationAction::StripPeriods,
        is_negative: false,
        left_strict: Option::None,
        right_strict: Option::None,
    });
    // 2 groups, point-separated, with comma
    patterns.push(CompoundTokenPattern {
        regex: Regex::new(
            r"\d+\.+\d+( , \d+|,\d+)"
        ).unwrap(),
        pattern_type: "numeric".into(),
        group: 0,
        priority: flatten_priority(&[2, 1, 3, 1]),
        normalization: NormalizationAction::StripPeriods,
        is_negative: false,
        left_strict: Option::None,
        right_strict: Option::None,
    });
    // 2 groups, point-separated, without comma
    patterns.push(CompoundTokenPattern {
        regex: Regex::new(
            r"\d+\.+\d+"
        ).unwrap(),
        pattern_type: "numeric".into(),
        group: 0,
        priority: flatten_priority(&[2, 1, 3, 2]),
        normalization: NormalizationAction::None,
        is_negative: false,
        left_strict: Option::None,
        right_strict: Option::None,
    });
    // 2 groups, space-separated
    patterns.push(CompoundTokenPattern {
        regex: Regex::new(
            r"\d+\ +\d\d\d+( , \d+|,\d+)?"
        ).unwrap(),
        pattern_type: "numeric".into(),
        group: 0,
        priority: flatten_priority(&[2, 1, 4]),
        normalization: NormalizationAction::StripWhitespace,
        is_negative: false,
        left_strict: Option::None,
        right_strict: Option::None,
    });
    // 1 group with comma or period-ending
    patterns.push(CompoundTokenPattern {
        regex: Regex::new(
            r"\d+( , \d+|,\d+| *\.)"
        ).unwrap(),
        pattern_type: "numeric".into(),
        group: 0,
        priority: flatten_priority(&[2, 1, 5]),
        normalization: NormalizationAction::StripWhitespace,
        is_negative: false,
        left_strict: Option::None,
        right_strict: Option::None,
    });

    // Roman numerals
    patterns.push(CompoundTokenPattern {
        regex: Regex::new(&format!(
            r"(^|\s)((I|II|III|IV|V|VI|VII|VIII|IX|X)\s*\.)\s*([{LOWERCASE}]|\d\d\d\d)",
            LOWERCASE = LOWERCASE
        )).unwrap(),
        pattern_type: "roman_numerals".into(),
        group: 2,
        priority: flatten_priority(&[2, 2, 0]),
        normalization: NormalizationAction::StripWhitespaceGroup(2),
        is_negative: false,
        left_strict: Option::None,
        right_strict: Option::None,
    });

    // === Unit patterns ===
    patterns.push(CompoundTokenPattern {
        regex: Regex::new(&format!(
            r"([0-9])\s*(([{LETTERS}]{{1,3}})\s?/\s?([{LETTERS}]{{1,3}}))",
            LETTERS = LETTERS
        )).unwrap(),
        pattern_type: "unit".into(),
        group: 2,
        priority: flatten_priority(&[3, 1]),
        normalization: NormalizationAction::StripWhitespaceGroup(2),
        is_negative: false,
        left_strict: Option::None,
        right_strict: Option::None,
    });
    patterns.push(CompoundTokenPattern {
        regex: Regex::new(&format!(
            r"(^|[^{LETTERS}])({UNITS})([^{LETTERS}]|$)",
            LETTERS = LETTERS,
            UNITS = UNITS
        )).unwrap(),
        pattern_type: "unit".into(),
        group: 2,
        priority: flatten_priority(&[3, 2]),
        normalization: NormalizationAction::StripWhitespaceGroup(2),
        is_negative: false,
        left_strict: Option::None,
        right_strict: Option::None,
    });

    // === Abbreviations before initials ===
    // P.S. / P.P.S.
    patterns.push(CompoundTokenPattern {
        regex: Regex::new(
            r"((P\s?\.\s?P\s?\.\s?S|P\s?\.\s?S)\s?\.)"
        ).unwrap(),
        pattern_type: "non_ending_abbreviation".into(),
        group: 1,
        priority: flatten_priority(&[4, 0, 0, 1]),
        normalization: NormalizationAction::CompactPeriods,
        is_negative: false,
        left_strict: Option::None,
        right_strict: Option::None,
    });
    patterns.push(CompoundTokenPattern {
        regex: Regex::new(
            r"(P\s?\.\s?P\s?\.\s?S|P\s?\.\s?S)"
        ).unwrap(),
        pattern_type: "non_ending_abbreviation".into(),
        group: 1,
        priority: flatten_priority(&[4, 0, 0, 2]),
        normalization: NormalizationAction::CompactPeriods,
        is_negative: false,
        left_strict: Option::None,
        right_strict: Option::None,
    });

    // === Initial patterns ===
    // Negative: temperature unit
    patterns.push(CompoundTokenPattern {
        regex: Regex::new(
            r"([º˚\u{00B0}]+\s*[CF])"
        ).unwrap(),
        pattern_type: "negative:temperature_unit".into(),
        group: 1,
        priority: flatten_priority(&[4, 0, 1]),
        normalization: NormalizationAction::StripWhitespaceGroup(1),
        is_negative: true,
        left_strict: Option::None,
        right_strict: Option::None,
    });
    // 2 initials + last name
    patterns.push(CompoundTokenPattern {
        regex: Regex::new(&format!(
            r"([{UPPERCASE}][{LOWERCASE}]?)\s?\.\s?\-?([{UPPERCASE}][{LOWERCASE}]?)\s?\.\s?((\.[{UPPERCASE}]\.)?[{UPPERCASE}][{LOWERCASE}]+)",
            UPPERCASE = UPPERCASE,
            LOWERCASE = LOWERCASE
        )).unwrap(),
        pattern_type: "name_with_initial".into(),
        group: 0,
        priority: flatten_priority(&[4, 1]),
        normalization: NormalizationAction::None,
        is_negative: false,
        left_strict: Option::None,
        right_strict: Option::None,
    });
    // 1 initial + last name
    patterns.push(CompoundTokenPattern {
        regex: Regex::new(&format!(
            r"([{UPPERCASE}])\s?\.\s?([{UPPERCASE}][{LOWERCASE}]+)",
            UPPERCASE = UPPERCASE,
            LOWERCASE = LOWERCASE
        )).unwrap(),
        pattern_type: "name_with_initial".into(),
        group: 0,
        priority: flatten_priority(&[4, 2]),
        normalization: NormalizationAction::None,
        is_negative: false,
        left_strict: Option::None,
        right_strict: Option::None,
    });

    // === Abbreviation patterns ===
    // Month name abbreviations
    patterns.push(CompoundTokenPattern {
        regex: Regex::new(&format!(
            r"[0-9]\.?\s*(([Jj]aan|[Vv]eebr?|Mär|[Aa]pr|Jun|Jul|[Aa]ug|[Ss]ept|[Oo]kt|[Nn]ov|[Dd]ets)\s?\.)\s*([{LOWERCASE}]|\d\d\d\d)",
            LOWERCASE = LOWERCASE
        )).unwrap(),
        pattern_type: "non_ending_abbreviation".into(),
        group: 1,
        priority: flatten_priority(&[5, 1, 0]),
        normalization: NormalizationAction::StripWhitespaceGroup(1),
        is_negative: false,
        left_strict: Option::None,
        right_strict: Option::None,
    });
    // Non-ending abbreviations with period
    patterns.push(CompoundTokenPattern {
        regex: Regex::new(&format!(
            r"(({ABBREVIATIONS1}|{ABBREVIATIONS2})\s?\.)",
            ABBREVIATIONS1 = ABBREVIATIONS1,
            ABBREVIATIONS2 = ABBREVIATIONS2
        )).unwrap(),
        pattern_type: "non_ending_abbreviation".into(),
        group: 1,
        priority: flatten_priority(&[5, 2, 0]),
        normalization: NormalizationAction::CompactPeriods,
        is_negative: false,
        left_strict: Option::None,
        right_strict: Option::None,
    });
    // Non-ending abbreviations without period
    patterns.push(CompoundTokenPattern {
        regex: Regex::new(&format!(
            r"({ABBREVIATIONS1}|{ABBREVIATIONS2})",
            ABBREVIATIONS1 = ABBREVIATIONS1,
            ABBREVIATIONS2 = ABBREVIATIONS2
        )).unwrap(),
        pattern_type: "non_ending_abbreviation".into(),
        group: 1,
        priority: flatten_priority(&[5, 3, 0]),
        normalization: NormalizationAction::CompactPeriods,
        is_negative: false,
        left_strict: Option::None,
        right_strict: Option::None,
    });
    // Abbreviations that can end the sentence (with period, 3A pattern)
    patterns.push(CompoundTokenPattern {
        regex: Regex::new(&format!(
            r"({ABBREVIATIONS3A}\s?\.)",
            ABBREVIATIONS3A = ABBREVIATIONS3A
        )).unwrap(),
        pattern_type: "abbreviation".into(),
        group: 1,
        priority: flatten_priority(&[5, 4, 0]),
        normalization: NormalizationAction::CompactPeriods,
        is_negative: false,
        left_strict: Option::None,
        right_strict: Option::None,
    });
    // Abbreviations that can end the sentence (without period, 3B pattern)
    patterns.push(CompoundTokenPattern {
        regex: Regex::new(&format!(
            r"({ABBREVIATIONS3B})",
            ABBREVIATIONS3B = ABBREVIATIONS3B
        )).unwrap(),
        pattern_type: "abbreviation".into(),
        group: 1,
        priority: flatten_priority(&[5, 5, 0]),
        normalization: NormalizationAction::CompactPeriods,
        is_negative: false,
        left_strict: Option::None,
        right_strict: Option::None,
    });
    // Letter + number abbreviations (e.g., E 251)
    patterns.push(CompoundTokenPattern {
        regex: Regex::new(&format!(
            r"([{UPPERCASE}](\s|\s?\-\s?)[0-9]+)",
            UPPERCASE = UPPERCASE
        )).unwrap(),
        pattern_type: "abbreviation".into(),
        group: 1,
        priority: flatten_priority(&[5, 6, 0]),
        normalization: NormalizationAction::StripWhitespaceGroup(1),
        is_negative: false,
        left_strict: Option::None,
        right_strict: Option::None,
    });

    patterns
}

/// Build all 2nd level (non-strict) patterns.
pub fn build_level2_patterns() -> Vec<CompoundTokenPattern> {
    let mut patterns = Vec::new();

    let case_endings = r"isse|li[sn]e|list|iks|ile|ilt|iga|ist|sse|ide|ina|ini|ita|il|it|le|lt|ga|st|is|ni|na|id|ed|ta|te|ks|se|ne|es|i|l|s|d|u|e|t";

    // Case endings pattern #1: word + separator + case ending (one separating space max)
    patterns.push(CompoundTokenPattern {
        regex: Regex::new(&format!(
            r"([{ALPHANUM}][.%\x22]?(\s[\-\'\u{{2032}}\u{{2019}}\u{{00B4}}]|[\-\'\u{{2032}}\u{{2019}}\u{{00B4}}]\s|[\-\'\u{{2032}}\u{{2019}}\u{{00B4}}`])({ENDINGS}))",
            ALPHANUM = ALPHANUM,
            ENDINGS = case_endings
        )).unwrap(),
        pattern_type: "case_ending".into(),
        group: 1,
        priority: flatten_priority(&[6, 0, 1]),
        normalization: NormalizationAction::StripWhitespaceGroup(1),
        is_negative: false,
        left_strict: Some(false),
        right_strict: Some(true),
    });

    // Case endings pattern #2: special case with two separating spaces
    patterns.push(CompoundTokenPattern {
        regex: Regex::new(&format!(
            r"([{ALPHANUM}][.%\x22]?(\s[\'\u{{2032}}\u{{2019}}\u{{00B4}}`]\s)({ENDINGS}))",
            ALPHANUM = ALPHANUM,
            ENDINGS = case_endings
        )).unwrap(),
        pattern_type: "case_ending".into(),
        group: 1,
        priority: flatten_priority(&[6, 0, 2]),
        normalization: NormalizationAction::StripWhitespaceGroup(1),
        is_negative: false,
        left_strict: Some(false),
        right_strict: Some(true),
    });

    // Case endings pattern #3: numeric + % or . + case ending
    patterns.push(CompoundTokenPattern {
        regex: Regex::new(&format!(
            r"([{NUMERIC}]\s?[.%]\s?({ENDINGS}))([^{ALPHANUM}]|$)",
            NUMERIC = NUMERIC,
            ALPHANUM = ALPHANUM,
            ENDINGS = case_endings
        )).unwrap(),
        pattern_type: "case_ending".into(),
        group: 1,
        priority: flatten_priority(&[6, 0, 3]),
        normalization: NormalizationAction::StripWhitespaceGroup(1),
        is_negative: false,
        left_strict: Some(false),
        right_strict: Some(true),
    });

    // Case endings pattern #4: numeric with unseparated case ending
    patterns.push(CompoundTokenPattern {
        regex: Regex::new(&format!(
            r"([{NUMERIC}]+[.,][{NUMERIC}]+(ks|le|lt|ga|st|sse|na|ni|ta|l|t|ne|es|i|l|s|d|u|e|t))",
            NUMERIC = NUMERIC
        )).unwrap(),
        pattern_type: "case_ending".into(),
        group: 1,
        priority: flatten_priority(&[6, 0, 4]),
        normalization: NormalizationAction::StripWhitespaceGroup(1),
        is_negative: false,
        left_strict: Some(true),
        right_strict: Some(true),
    });

    // Case endings pattern #5: year/decade + .- + nda-ending
    patterns.push(CompoundTokenPattern {
        regex: Regex::new(&format!(
            r"([{NUMERIC}]\s?([.]\s?\-|\-|[.])\s?nda(tesse|teks|teni|test|tena|tele|telt|tega|isse|iks|ile|ilt|ist|sse|ina|ini|ita|tel|tes|il|le|lt|ga|st|is|ni|na|id|ta|te|ks|l|s|d|t)?)([^{ALPHANUM}]|$)",
            NUMERIC = NUMERIC,
            ALPHANUM = ALPHANUM
        )).unwrap(),
        pattern_type: "case_ending".into(),
        group: 1,
        priority: flatten_priority(&[6, 0, 5]),
        normalization: NormalizationAction::StripWhitespaceGroup(1),
        is_negative: false,
        left_strict: Some(false),
        right_strict: Some(true),
    });

    // Number fixes: sign (+ or -)
    patterns.push(CompoundTokenPattern {
        regex: Regex::new(&format!(
            r"(^|[^{NUMERIC}. ])\s*((\+/\-|[\-+±\u{{2013}}])[{NUMERIC}])",
            NUMERIC = NUMERIC
        )).unwrap(),
        pattern_type: "sign".into(),
        group: 2,
        priority: flatten_priority(&[7, 0, 1]),
        normalization: NormalizationAction::StripWhitespaceGroup(2),
        is_negative: false,
        left_strict: Some(true),
        right_strict: Some(false),
    });

    // Number fixes: percentage
    patterns.push(CompoundTokenPattern {
        regex: Regex::new(&format!(
            r"([{NUMERIC}]\s*(\-protsendi[^\s]+|%))",
            NUMERIC = NUMERIC
        )).unwrap(),
        pattern_type: "percentage".into(),
        group: 1,
        priority: flatten_priority(&[7, 0, 2]),
        normalization: NormalizationAction::StripWhitespaceGroup(1),
        is_negative: false,
        left_strict: Some(false),
        right_strict: Some(true),
    });

    patterns
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_level1_patterns_build() {
        let patterns = build_level1_patterns();
        assert!(patterns.len() > 30, "Expected 30+ patterns, got {}", patterns.len());
    }

    #[test]
    fn test_level2_patterns_build() {
        let patterns = build_level2_patterns();
        assert!(patterns.len() >= 7, "Expected 7+ patterns, got {}", patterns.len());
    }

    #[test]
    fn test_date_pattern() {
        let patterns = build_level1_patterns();
        let date_pat = patterns.iter().find(|p| p.pattern_type == "numeric_date").unwrap();
        assert!(date_pat.regex.is_match("02.02.2010"));
    }

    #[test]
    fn test_email_pattern() {
        let patterns = build_level1_patterns();
        let email_pat = patterns.iter().find(|p| p.pattern_type == "email").unwrap();
        assert!(email_pat.regex.is_match("user@example.com"));
    }
}
