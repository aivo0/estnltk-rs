// Estonian character class constants for regex patterns.
// These map to the MACROS dictionary in Python patterns.py.

/// Estonian lowercase letters including š, ž, õ, ä, ö, ü
pub const LOWERCASE: &str = "a-zšžõäöü";

/// Estonian uppercase letters including Š, Ž, Õ, Ä, Ö, Ü
pub const UPPERCASE: &str = "A-ZŠŽÕÄÖÜ";

/// Digits 0-9
pub const NUMERIC: &str = "0-9";

/// All Estonian letters (lower + upper)
pub const LETTERS: &str = "a-zšžõäöüA-ZŠŽÕÄÖÜ";

/// Alphanumeric: letters + digits
pub const ALPHANUM: &str = "a-zšžõäöüA-ZŠŽÕÄÖÜ0-9";

// ============================================================
//   Abbreviation pattern fragments
// ============================================================

/// Non-ending abbreviations that may be affected by tokenization (split further into tokens)
/// These are longer patterns and should be checked first
pub const ABBREVIATIONS1: &str = concat!(
    "(",
    r"a\s?\.\s?k\s?\.\s?a|",
    r"n\s?\.\s?-\s?ö|",
    "a['`'\u{2019} ]la|",
    r"k\s?\.\s?a|",
    r"n\s?\.\s?ö|",
    r"n\s?\.\s?n|",
    r"s\s?\.\s?o|",
    r"s\s?\.\s?t|",
    r"s\s?\.\s?h|",
    r"v\s?\.\s?a",
    ")"
);

/// Non-ending abbreviations that should come out of tokenization as they are
pub const ABBREVIATIONS2: &str = concat!(
    "(",
    "ca|[Dd]r|[Hh]r|[Hh]rl|[Ii]bid|[Kk]od|[Kk]oost|[Ll]p|",
    "lüh|[Mm]rs?|nn|[Nn]r|[Nn]t|nö|[Pp]r|sealh|so|st|sh|",
    "[Ss]m|[Tt]lk|tn|[Tt]oim|[Vv]rd|va|[Vv]t",
    ")"
);

/// Abbreviations that can end the sentence (with single-letter patterns, checked for ending period)
pub const ABBREVIATIONS3A: &str = concat!(
    "(",
    r"e\s?\.\s?m\s?\.\s?a|",
    r"m\s?\.\s?a\s?\.\s?j|",
    r"e\s?\.\s?Kr|",
    r"p\s?\.\s?Kr|",
    r"A\s?\.\s?D|",
    r"õ\s?\.\s?a|",
    "saj|",
    "[Jj]r|",
    "j[mt]|",
    "a|",
    "u",
    ")"
);

/// Abbreviations that can end the sentence (without single-letter patterns)
pub const ABBREVIATIONS3B: &str = concat!(
    "(",
    r"e\s?\.\s?m\s?\.\s?a|",
    r"m\s?\.\s?a\s?\.\s?j|",
    r"e\s?\.\s?Kr|",
    r"p\s?\.\s?Kr|",
    r"A\s?\.\s?D|",
    r"õ\s?\.\s?a|",
    "saj|",
    "[Jj]r|",
    "j[mt]",
    ")"
);

/// Common unit combinations
pub const UNITS: &str = concat!(
    "(",
    r"l\s?/\s?(min|sek)|",
    r"m\s?/\s?(min|sek|[st])|",
    r"km\s?/\s?[hst]",
    ")"
);
