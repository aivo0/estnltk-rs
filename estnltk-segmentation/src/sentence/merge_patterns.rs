use regex::Regex;

/// A merge pattern that describes when two adjacent sentences should be joined.
pub struct MergePattern {
    /// Regex for the end of the previous sentence
    pub begin_pat: Regex,
    /// Regex for the start of the next sentence
    pub end_pat: Regex,
    /// Classification of the fix type
    pub fix_type: String,
    /// If true, the sentence ending needs to be shifted to a named group <end>
    pub shift_end: bool,
    /// Optional negative pattern: if this matches, the end_pat is considered NOT matched.
    /// Used to emulate negative lookahead which the regex crate doesn't support.
    pub end_pat_negate: Option<Regex>,
}

impl MergePattern {
    /// Check if the end pattern matches (accounting for negation).
    pub fn end_matches(&self, text: &str) -> bool {
        if !self.end_pat.is_match(text) {
            return false;
        }
        if let Some(ref negate) = self.end_pat_negate {
            return !negate.is_match(text);
        }
        true
    }
}

// Estonian character patterns used in merge rules
const HYPHEN_PAT: &str = r"(\u{2212}|\u{FF0D}|\u{02D7}|\u{FE63}|\u{002D}|\u{2010}|\u{2011}|\u{2012}|\u{2013}|\u{2014}|\u{2015})";
const LC_LETTER: &str = "[a-z\u{00F6}\u{00E4}\u{00FC}\u{00F5}\u{017E}\u{0161}]";
const UC_LETTER: &str = "[A-Z\u{00D6}\u{00C4}\u{00DC}\u{00D5}\u{017D}\u{0160}]";
const NOT_LETTER: &str = "[^A-Z\u{00D6}\u{00C4}\u{00DC}\u{00D5}\u{017D}\u{0160}a-z\u{00F6}\u{00E4}\u{00FC}\u{00F5}\u{017E}\u{0161}]";
const START_QUOTES: &str = "\"\u{00AB}\u{02EE}\u{030B}\u{201C}\u{201E}";
const ENDING_QUOTES: &str = "\"\u{00BB}\u{02EE}\u{030B}\u{201D}\u{201E}";

/// Build all merge patterns (port of Python's merge_patterns list).
pub fn build_merge_patterns() -> Vec<MergePattern> {
    let mut patterns = Vec::new();

    // === Numeric range fixes ===
    // {Numeric_range_start} {period} + {dash} {Numeric_range_end}
    patterns.push(MergePattern {
        begin_pat: Regex::new(&format!(r"(?s)(.+)?([0-9]+)\s*\.$")).unwrap(),
        end_pat: Regex::new(&format!(r"(?s){HYPHEN}\s*([0-9]+)\s*\.(.*)?$", HYPHEN = HYPHEN_PAT)).unwrap(),
        fix_type: "numeric_range".into(),
        shift_end: false,
        end_pat_negate: None,
    });
    // {Numeric_range_start} {period} {dash} + {Numeric_range_end}
    patterns.push(MergePattern {
        begin_pat: Regex::new(&format!(r"(?s)(.+)?([0-9]+)\s*\.\s*{HYPHEN}$", HYPHEN = HYPHEN_PAT)).unwrap(),
        end_pat: Regex::new(r"(?s)([0-9]+)\s*\.(.+)?$").unwrap(),
        fix_type: "numeric_range".into(),
        shift_end: false,
        end_pat_negate: None,
    });

    // === Numeric year fixes ===
    // {Numeric_year} {period} {|a|} + {lowercase_or_number}
    patterns.push(MergePattern {
        begin_pat: Regex::new(r"(?s)(.+)?([0-9]{3,4})\s*\.?\s*a\s*\.$").unwrap(),
        end_pat: Regex::new(&format!("^({LC}|[0-9])+", LC = LC_LETTER)).unwrap(),
        fix_type: "numeric_year".into(),
        shift_end: false,
        end_pat_negate: None,
    });
    patterns.push(MergePattern {
        begin_pat: Regex::new(r"(?s)(^|.+)([0-9]{4}\s*\.?|/\s*[0-9]{2})\s*\u{00F5}\s*\.?\s*a\.?$").unwrap(),
        end_pat: Regex::new(&format!("^({LC}|[0-9])+", LC = LC_LETTER)).unwrap(),
        fix_type: "numeric_year".into(),
        shift_end: false,
        end_pat_negate: None,
    });
    // {Numeric_year} {period} + {|a|} {lowercase_or_number}
    patterns.push(MergePattern {
        begin_pat: Regex::new(r"(?s)(.+)?([0-9]{4})\s*\.$").unwrap(),
        end_pat: Regex::new(&format!(r"^\s*(\u{{00F5}}\s*\.)?a\.?\s*({LC}|[0-9])+", LC = LC_LETTER)).unwrap(),
        fix_type: "numeric_year".into(),
        shift_end: false,
        end_pat_negate: None,
    });
    // {Numeric_year} {period} + {|aasta|}
    patterns.push(MergePattern {
        begin_pat: Regex::new(r"(?s)(.+)?([0-9]{3,4})\s*\.$").unwrap(),
        end_pat: Regex::new(&format!("^{LC}*aasta.*", LC = LC_LETTER)).unwrap(),
        fix_type: "numeric_year".into(),
        shift_end: false,
        end_pat_negate: None,
    });
    // {Numeric|Roman_numeral_century} {period} {|sajand|} + {lowercase}
    patterns.push(MergePattern {
        begin_pat: Regex::new(r"(?s)(.+)?([0-9]{1,2}|[IVXLCDM]+)\s*\.?\s*saj\.?$").unwrap(),
        end_pat: Regex::new(&format!("^{LC}+", LC = LC_LETTER)).unwrap(),
        fix_type: "numeric_century".into(),
        shift_end: false,
        end_pat_negate: None,
    });

    // === Date/time fixes ===
    // {Date_dd.mm.yyyy.} + {time_HH:MM}
    patterns.push(MergePattern {
        begin_pat: Regex::new(r"(?s)(.+)?([0-9]{2})\.([0-9]{2})\.([0-9]{4})\.\s*$").unwrap(),
        end_pat: Regex::new(r"^\s*([0-9]{2}):([0-9]{2})").unwrap(),
        fix_type: "numeric_date".into(),
        shift_end: false,
        end_pat_negate: None,
    });
    // {|kell|} {time_HH.} + {MM}
    patterns.push(MergePattern {
        begin_pat: Regex::new(r"(?s)(.+)?[kK]ell\S?\s([0-9]{1,2})\s*\.\s*$").unwrap(),
        end_pat: Regex::new(r"^\s*([0-9]{2})\s").unwrap(),
        fix_type: "numeric_time".into(),
        shift_end: false,
        end_pat_negate: None,
    });

    // {Numeric_date} {period} + {month_name}
    patterns.push(MergePattern {
        begin_pat: Regex::new(r"(?s)(.+)?([0-9]{1,2})\s*\.$").unwrap(),
        end_pat: Regex::new("^(jaan|veeb|m\u{00E4}rts|apr|mai|juul|juun|augu|septe|okto|nove|detse).*").unwrap(),
        fix_type: "numeric_date".into(),
        shift_end: false,
        end_pat_negate: None,
    });
    // {Numeric_date} {period} + {month_name_short}
    patterns.push(MergePattern {
        begin_pat: Regex::new(r"(?s)(.+)?([0-9]{1,2})\s*\.$").unwrap(),
        end_pat: Regex::new(r"^(jaan|veebr?|m\u{00E4}r|apr|mai|juul|juun|aug|sept|okt|nov|dets)(\s*\.|\s).*").unwrap(),
        fix_type: "numeric_date".into(),
        shift_end: false,
        end_pat_negate: None,
    });
    // {Month_name_short} {period} + {numeric_year}
    patterns.push(MergePattern {
        begin_pat: Regex::new(r"(?s)^(.+)?\s(jaan|veebr?|m\u{00E4}r|apr|mai|juul|juun|aug|sept|okt|nov|dets)\s*\..*").unwrap(),
        end_pat: Regex::new(r"([0-9]{4})\s*.*").unwrap(),
        fix_type: "numeric_date".into(),
        shift_end: false,
        end_pat_negate: None,
    });

    // === Roman numeral fixes ===
    patterns.push(MergePattern {
        begin_pat: Regex::new(r"(?s)(.+)?((VIII|III|VII|II|IV|VI|IX|V|I|X)\s*\.)$").unwrap(),
        end_pat: Regex::new(&format!("^({LC}|{HYPHEN})", LC = LC_LETTER, HYPHEN = HYPHEN_PAT)).unwrap(),
        fix_type: "numeric_roman_numeral".into(),
        shift_end: false,
        end_pat_negate: None,
    });

    // === Number + period + lowercase ===
    patterns.push(MergePattern {
        begin_pat: Regex::new(r"(?s)(.+)?([0-9]+)\s*\.$").unwrap(),
        end_pat: Regex::new(&format!("^{LC}+", LC = LC_LETTER)).unwrap(),
        fix_type: "numeric_ordinal_numeral".into(),
        shift_end: false,
        end_pat_negate: None,
    });
    // {Number} {period} + {hyphen}
    patterns.push(MergePattern {
        begin_pat: Regex::new(r"(?s)(.+)?([0-9]+)\s*\.$").unwrap(),
        end_pat: Regex::new(&format!("^{HYPHEN}+", HYPHEN = HYPHEN_PAT)).unwrap(),
        fix_type: "numeric_monetary".into(),
        shift_end: false,
        end_pat_negate: None,
    });

    // === Abbreviation fixes ===
    // BCE period
    patterns.push(MergePattern {
        begin_pat: Regex::new(r"(?s)(.+)?[pe]\s*\.\s*Kr\s*\.?$").unwrap(),
        end_pat: Regex::new(&format!("^{LC}+", LC = LC_LETTER)).unwrap(),
        fix_type: "abbrev_century".into(),
        shift_end: false,
        end_pat_negate: None,
    });
    // Abbreviation + period + numeric
    patterns.push(MergePattern {
        begin_pat: Regex::new(r"(?s)(.+)?([Ll]k|[Nn]r)\s*\.$").unwrap(),
        end_pat: Regex::new("^[0-9]+").unwrap(),
        fix_type: "abbrev_numeric".into(),
        shift_end: false,
        end_pat_negate: None,
    });
    // Common abbreviation + period + lowercase/hyphen/comma/)
    patterns.push(MergePattern {
        begin_pat: Regex::new(
            r"(?s)(.+)?\s(ingl|n\u{00E4}it|jm[st]|jne|jp[mt]|mnt|pst|tbl|vm[st]|j[tm]|mh|vm|e|t)\s?[.]$"
        ).unwrap(),
        end_pat: Regex::new(&format!(r"^({LC}|{HYPHEN}|,|;|\))", LC = LC_LETTER, HYPHEN = HYPHEN_PAT)).unwrap(),
        fix_type: "abbrev_common".into(),
        shift_end: false,
        end_pat_negate: None,
    });
    // abbreviation + period + comma_or_semicolon
    patterns.push(MergePattern {
        begin_pat: Regex::new(r"(?s).+\s[a-z\u{00F6}\u{00E4}\u{00FC}\u{00F5}\-.]+[.]\s*$").unwrap(),
        end_pat: Regex::new("^([,;]).*").unwrap(),
        fix_type: "abbrev_common".into(),
        shift_end: false,
        end_pat_negate: None,
    });
    // uppercase_letter + period + not_uppercase_followed_by_lowercase
    // Uses end_pat_negate to emulate negative lookahead (?!\s*UC LC)
    patterns.push(MergePattern {
        begin_pat: Regex::new(&format!(
            r"(?s)(.*{NOT_LETTER}|^){UC}\s*[.]\s*$",
            NOT_LETTER = NOT_LETTER,
            UC = UC_LETTER
        )).unwrap(),
        end_pat: Regex::new(r"^.*").unwrap(),
        fix_type: "abbrev_name_initial".into(),
        shift_end: false,
        end_pat_negate: Some(Regex::new(&format!(
            r"^\s*{UC}{LC}",
            UC = UC_LETTER,
            LC = LC_LETTER
        )).unwrap()),
    });

    // === Parentheses fixes ===
    // period_ending_content_of_parentheses + lowercase_or_comma
    patterns.push(MergePattern {
        begin_pat: Regex::new(r"(?s)(.+)?\([^()]+[.!]\s*\)$").unwrap(),
        end_pat: Regex::new(&format!("^({LC}|,)+.*", LC = LC_LETTER)).unwrap(),
        fix_type: "parentheses".into(),
        shift_end: false,
        end_pat_negate: None,
    });
    // parentheses_start + content + parentheses_end (no uppercase)
    patterns.push(MergePattern {
        begin_pat: Regex::new(r"(?s).*\([^()]+$").unwrap(),
        end_pat: Regex::new(r"^[^()A-Z\u{00D6}\u{00C4}\u{00DC}\u{00D5}\u{0160}\u{017D}]*\)\s*$").unwrap(),
        fix_type: "parentheses".into(),
        shift_end: false,
        end_pat_negate: None,
    });
    patterns.push(MergePattern {
        begin_pat: Regex::new(r"(?s).*\([^()]+$").unwrap(),
        end_pat: Regex::new(r"^[^()A-Z\u{00D6}\u{00C4}\u{00DC}\u{00D5}\u{0160}\u{017D}]*\)(\s|\n)*[^A-Z\u{00D6}\u{00C4}\u{00DC}\u{00D5}\u{0160}\u{017D} \n].*").unwrap(),
        fix_type: "parentheses".into(),
        shift_end: false,
        end_pat_negate: None,
    });
    // ending_punctuation + parentheses_end<end> uppercase
    patterns.push(MergePattern {
        begin_pat: Regex::new(r"(?s).*[.!?]\s*$").unwrap(),
        end_pat: Regex::new(r"^(?P<end>\))(\s|\n)*[A-Z\u{00D6}\u{00C4}\u{00DC}\u{00D5}\u{0160}\u{017D}].*").unwrap(),
        fix_type: "parentheses".into(),
        shift_end: true,
        end_pat_negate: None,
    });
    // parentheses + lowercase content + closing paren
    patterns.push(MergePattern {
        begin_pat: Regex::new(r"(?s)(.+)?\([^()]+$").unwrap(),
        end_pat: Regex::new(&format!(r"^({LC}|,)[^()]+\).*", LC = LC_LETTER)).unwrap(),
        fix_type: "parentheses".into(),
        shift_end: false,
        end_pat_negate: None,
    });
    // parentheses + numeric patterns + closing paren
    patterns.push(MergePattern {
        begin_pat: Regex::new(r"(?s).*\([^()]+$").unwrap(),
        end_pat: Regex::new(r"^[0-9.\- ]+\).*").unwrap(),
        fix_type: "parentheses".into(),
        shift_end: false,
        end_pat_negate: None,
    });
    // content_in_parentheses + single_sentence_ending_symbol
    patterns.push(MergePattern {
        begin_pat: Regex::new(r"(?s).*\([^()]+\)$").unwrap(),
        end_pat: Regex::new(r"^[.?!\u{2026}]+$").unwrap(),
        fix_type: "parentheses".into(),
        shift_end: false,
        end_pat_negate: None,
    });

    // === Double quotes fixes ===
    // sentence_ending_punct + ending_quotes + comma_or_semicolon_or_lowercase
    patterns.push(MergePattern {
        begin_pat: Regex::new(r"(?s).+[?!.\u{2026}]\s*$").unwrap(),
        end_pat: Regex::new(&format!(r"^[{EQ}]\s*([,;]|{LC})+", EQ = ENDING_QUOTES, LC = LC_LETTER)).unwrap(),
        fix_type: "double_quotes".into(),
        shift_end: false,
        end_pat_negate: None,
    });
    patterns.push(MergePattern {
        begin_pat: Regex::new(&format!(r"(?s).+[?!.\u{{2026}}]\s*[{EQ}]$", EQ = ENDING_QUOTES)).unwrap(),
        end_pat: Regex::new(&format!("^([,;]|{LC})+", LC = LC_LETTER)).unwrap(),
        fix_type: "double_quotes".into(),
        shift_end: false,
        end_pat_negate: None,
    });
    // sentence_ending_punct + only_ending_quotes
    patterns.push(MergePattern {
        begin_pat: Regex::new(r"(?s).+?[?!.\u{2026}]$").unwrap(),
        end_pat: Regex::new(&format!("^[{EQ}]$", EQ = ENDING_QUOTES)).unwrap(),
        fix_type: "double_quotes".into(),
        shift_end: false,
        end_pat_negate: None,
    });
    // ending_punctuation + ending_quotes<end> starting_quotes
    patterns.push(MergePattern {
        begin_pat: Regex::new(r"(?s).*[.!?]\s*$").unwrap(),
        end_pat: Regex::new(&format!(r"^(?P<end>[{EQ}])(\s|\n)*[{SQ}].*", EQ = ENDING_QUOTES, SQ = START_QUOTES)).unwrap(),
        fix_type: "double_quotes".into(),
        shift_end: true,
        end_pat_negate: None,
    });
    // ending_punctuation + ending_quotes<end> starting_brackets
    patterns.push(MergePattern {
        begin_pat: Regex::new(r"(?s).*[.!?]\s*$").unwrap(),
        end_pat: Regex::new(&format!(r"^(?P<end>[{EQ}])(\s|\n)*[()\[].*", EQ = ENDING_QUOTES)).unwrap(),
        fix_type: "double_quotes".into(),
        shift_end: true,
        end_pat_negate: None,
    });
    // sentence_ending_punct + ending_quotes + only_sentence_ending_punct
    patterns.push(MergePattern {
        begin_pat: Regex::new(r"(?s).+[?!.\u{2026}]\s*$").unwrap(),
        end_pat: Regex::new(&format!(r"^[{EQ}]\s*[?!.\u{{2026}}]+$", EQ = ENDING_QUOTES)).unwrap(),
        fix_type: "double_quotes".into(),
        shift_end: false,
        end_pat_negate: None,
    });
    patterns.push(MergePattern {
        begin_pat: Regex::new(&format!(r"(?s).+[?!.\u{{2026}}]\s*[{EQ}]$", EQ = ENDING_QUOTES)).unwrap(),
        end_pat: Regex::new(r"^[.?!\u{2026}]+$").unwrap(),
        fix_type: "double_quotes".into(),
        shift_end: false,
        end_pat_negate: None,
    });

    // === Repeated ending punctuation ===
    patterns.push(MergePattern {
        begin_pat: Regex::new(r"(?s).+[?!.\u{2026}]\s*$").unwrap(),
        end_pat: Regex::new(r"^[.?!\u{2026}]+$").unwrap(),
        fix_type: "repeated_ending_punct".into(),
        shift_end: false,
        end_pat_negate: None,
    });

    // === Inner title punctuation ===
    patterns.push(MergePattern {
        begin_pat: Regex::new(r"(?s).+[?!]\s*$").unwrap(),
        end_pat: Regex::new(&format!(r"^([,;])\s*{LC}+", LC = LC_LETTER)).unwrap(),
        fix_type: "inner_title_punct".into(),
        shift_end: false,
        end_pat_negate: None,
    });

    patterns
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_build_merge_patterns() {
        let patterns = build_merge_patterns();
        assert!(patterns.len() >= 25, "Expected 25+ merge patterns, got {}", patterns.len());
    }

    #[test]
    fn test_numeric_range_pattern() {
        let patterns = build_merge_patterns();
        let pat = &patterns[0]; // first numeric_range pattern
        assert!(pat.begin_pat.is_match("P\u{00E4}evad 14."));
        assert!(pat.end_pat.is_match("- 17. aprillil."));
    }
}
