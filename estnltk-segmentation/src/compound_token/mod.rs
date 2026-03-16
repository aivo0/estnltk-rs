pub mod pattern_types;
pub mod patterns;

use std::collections::HashSet;

use regex::Regex;

use estnltk_core::{byte_to_char_map, char_to_byte_map, keep_maximal_matches, MatchEntry, MatchSpan};

use self::pattern_types::{CompoundTokenPattern, NormalizationAction};
use self::patterns::{build_level1_patterns, build_level2_patterns};

/// A detected compound token.
#[derive(Debug, Clone)]
pub struct CompoundToken {
    /// Bounding span of the compound token (character offsets)
    pub span: MatchSpan,
    /// The elementary token spans that make up this compound token
    pub token_spans: Vec<MatchSpan>,
    /// Type labels (e.g., ["numeric_date"], ["hyphenation", "case_ending"])
    pub pattern_type: Vec<String>,
    /// Normalized form, if any
    pub normalized: Option<String>,
}

/// Configuration for which pattern categories to enable.
#[derive(Debug, Clone)]
pub struct CompoundTokenConfig {
    pub tag_numbers: bool,
    pub tag_units: bool,
    pub tag_email_and_www: bool,
    pub tag_emoticons: bool,
    pub tag_hashtags_and_usernames: bool,
    pub tag_xml: bool,
    pub tag_initials: bool,
    pub tag_abbreviations: bool,
    pub tag_case_endings: bool,
    pub tag_hyphenations: bool,
    /// Strings that cancel compound token creation if found inside the token
    pub do_not_join_on_strings: Vec<String>,
}

impl Default for CompoundTokenConfig {
    fn default() -> Self {
        Self {
            tag_numbers: true,
            tag_units: true,
            tag_email_and_www: true,
            tag_emoticons: true,
            tag_hashtags_and_usernames: false,
            tag_xml: true,
            tag_initials: true,
            tag_abbreviations: true,
            tag_case_endings: true,
            tag_hyphenations: true,
            do_not_join_on_strings: vec!["\n\n".to_string()],
        }
    }
}

/// State for the hyphenation detection state machine.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum HyphenState {
    /// No hyphenation pattern in progress
    None,
    /// Just saw a hyphen adjacent to previous token
    Hyphen,
    /// Saw word after hyphen
    Second,
    /// Pattern ended, needs to be checked
    End,
}

/// The compound token tagger.
pub struct CompoundTokenTagger {
    level1_patterns: Vec<CompoundTokenPattern>,
    level2_patterns: Vec<CompoundTokenPattern>,
    config: CompoundTokenConfig,
    letter_re: Regex,
    only_hyphens_re: Regex,
    // Pre-compiled normalization regexes
    month_year_re: Regex,
    day_month_re: Regex,
    compact_period_re1: Regex,
    compact_period_re2: Regex,
    collapse_ws_re: Regex,
}

impl CompoundTokenTagger {
    pub fn new(config: CompoundTokenConfig) -> Self {
        let all_level1 = build_level1_patterns();
        let all_level2 = build_level2_patterns();

        // Filter patterns based on config
        let level1_patterns: Vec<CompoundTokenPattern> = all_level1
            .into_iter()
            .filter(|p| !p.is_negative && is_pattern_allowed(&p.pattern_type, &config))
            .collect();

        let level2_patterns: Vec<CompoundTokenPattern> = all_level2
            .into_iter()
            .filter(|p| is_level2_pattern_allowed(&p.pattern_type, &config))
            .collect();

        let letter_re = Regex::new(&format!(
            "[{}{}]+",
            crate::estonian::LOWERCASE,
            crate::estonian::UPPERCASE
        )).unwrap();
        let only_hyphens_re = Regex::new(r"^(-{2,})$").unwrap();

        let month_year_re = Regex::new(r"^([012][0-9]|1[012])\.(1[7-9]\d\d|2[0-2]\d\d)$").unwrap();
        let day_month_re = Regex::new(r"^(3[01]|[12][0-9]|0?[0-9])\.([012][0-9]|1[012])$").unwrap();
        let compact_period_re1 = Regex::new(r"\.\s").unwrap();
        let compact_period_re2 = Regex::new(r"\s\.").unwrap();
        let collapse_ws_re = Regex::new(r"\s{2,}").unwrap();

        CompoundTokenTagger {
            level1_patterns,
            level2_patterns,
            config,
            letter_re,
            only_hyphens_re,
            month_year_re,
            day_month_re,
            compact_period_re1,
            compact_period_re2,
            collapse_ws_re,
        }
    }

    /// Create a tagger with default configuration.
    pub fn estonian() -> Self {
        Self::new(CompoundTokenConfig::default())
    }

    /// Detect compound tokens in the given text.
    ///
    /// `tokens` are the character-level spans from TokensTagger.
    pub fn detect(&self, text: &str, tokens: &[MatchSpan]) -> Vec<CompoundToken> {
        let c2b = char_to_byte_map(text);
        let b2c = byte_to_char_map(text);

        // Step 1: Level 1 pattern matching (strict tokenization hints)
        let mut compound_tokens = self.apply_level1(text, tokens, &c2b, &b2c);

        // Step 2: Hyphenation correction
        if self.config.tag_hyphenations {
            let hyphenation_compounds = self.detect_hyphenations(text, tokens, &c2b);
            compound_tokens.extend(hyphenation_compounds);
        }

        // Sort by start position
        compound_tokens.sort_by(|a, b| a.span.start.cmp(&b.span.start).then(a.span.end.cmp(&b.span.end)));

        // Step 3: Level 2 pattern matching (non-strict hints)
        if !self.level2_patterns.is_empty() {
            compound_tokens = self.apply_level2(text, tokens, &compound_tokens, &c2b, &b2c);
        }

        // Final conflict resolution: keep maximal matches
        let entries: Vec<MatchEntry> = compound_tokens
            .iter()
            .enumerate()
            .map(|(i, ct)| (ct.span, i))
            .collect();
        let maximal = keep_maximal_matches(&entries);
        let kept_indices: HashSet<usize> = maximal.iter().map(|&(_, idx)| idx).collect();
        compound_tokens
            .into_iter()
            .enumerate()
            .filter(|(i, _)| kept_indices.contains(i))
            .map(|(_, ct)| ct)
            .collect()
    }

    /// Apply level 1 patterns against the full text.
    fn apply_level1(
        &self,
        text: &str,
        tokens: &[MatchSpan],
        _c2b: &[usize],
        b2c: &[usize],
    ) -> Vec<CompoundToken> {
        let mut hints: Vec<(MatchSpan, String, Option<String>, i64)> = Vec::new();

        for pattern in &self.level1_patterns {
            for caps in pattern.regex.captures_iter(text) {
                let m = caps.get(0).unwrap();
                let byte_start = m.start();
                let byte_end = m.end();
                let char_start = b2c[byte_start];
                let char_end = b2c[byte_end];

                // Get the match text for the specified group
                let match_text = &text[byte_start..byte_end];

                // Check for disallowed strings
                if self.contains_disallowed(match_text) {
                    continue;
                }

                // Extract group span from captures directly (no re-running regex)
                let (group_start, group_end) = if pattern.group == 0 {
                    (char_start, char_end)
                } else if let Some(grp) = caps.get(pattern.group) {
                    (b2c[grp.start()], b2c[grp.end()])
                } else {
                    continue;
                };

                let normalized = self.apply_normalization(&pattern.normalization, text, byte_start, byte_end, &pattern.regex);

                hints.push((
                    MatchSpan::new(group_start, group_end),
                    pattern.pattern_type.clone(),
                    normalized,
                    pattern.priority,
                ));
            }
        }

        // Sort by (start, end, priority) for conflict resolution
        hints.sort_by(|a, b| {
            a.0.start.cmp(&b.0.start)
                .then(a.0.end.cmp(&b.0.end))
                .then(a.3.cmp(&b.3))
        });

        // Deduplicate: for hints with the same span, keep only the lowest priority
        hints.dedup_by(|b, a| a.0 == b.0);

        // Keep maximal matches
        let entries: Vec<MatchEntry> = hints
            .iter()
            .enumerate()
            .map(|(i, h)| (h.0, i))
            .collect();
        let maximal = keep_maximal_matches(&entries);
        let kept: HashSet<usize> = maximal.iter().map(|&(_, i)| i).collect();

        // Map hint spans to token spans and build compound tokens
        let mut result = Vec::new();
        for (i, (span, pattern_type, normalized, _priority)) in hints.into_iter().enumerate() {
            if !kept.contains(&i) {
                continue;
            }
            // Find tokens covered by this hint span (binary search + scan)
            let start_idx = tokens.partition_point(|t| t.start < span.start);
            let covered: Vec<MatchSpan> = tokens[start_idx..]
                .iter()
                .take_while(|t| t.start < span.end)
                .filter(|t| t.end <= span.end)
                .copied()
                .collect();
            if covered.is_empty() {
                continue;
            }
            // Verify end alignment
            let last_token_end = covered.last().map(|t| t.end).unwrap_or(0);
            let first_token_start = covered.first().map(|t| t.start).unwrap_or(0);
            if last_token_end != span.end {
                // Try to find the matching end token
                let adjusted_covered: Vec<MatchSpan> = tokens[start_idx..]
                    .iter()
                    .take_while(|t| t.start < span.end)
                    .copied()
                    .collect();
                if adjusted_covered.len() >= 2 {
                    result.push(CompoundToken {
                        span: MatchSpan::new(
                            adjusted_covered.first().unwrap().start,
                            adjusted_covered.last().unwrap().end,
                        ),
                        token_spans: adjusted_covered,
                        pattern_type: vec![pattern_type],
                        normalized,
                    });
                }
                continue;
            }
            if covered.len() >= 2 {
                result.push(CompoundToken {
                    span: MatchSpan::new(first_token_start, last_token_end),
                    token_spans: covered,
                    pattern_type: vec![pattern_type],
                    normalized,
                });
            }
        }

        result
    }

    /// Detect hyphenated words by scanning for word-hyphen-word adjacency patterns.
    fn detect_hyphenations(
        &self,
        text: &str,
        tokens: &[MatchSpan],
        c2b: &[usize],
    ) -> Vec<CompoundToken> {
        let mut result = Vec::new();
        if tokens.is_empty() {
            return result;
        }

        let mut hyphenation_start = 0;
        let mut state = HyphenState::None;
        let mut last_end = 0;

        for (i, &token) in tokens.iter().enumerate() {
            let token_text = &text[c2b[token.start]..c2b[token.end]];

            match state {
                HyphenState::None => {
                    if last_end == token.start && token_text == "-" {
                        state = HyphenState::Hyphen;
                    } else {
                        hyphenation_start = i;
                    }
                }
                HyphenState::Hyphen => {
                    if last_end == token.start {
                        state = HyphenState::Second;
                    } else {
                        state = HyphenState::End;
                    }
                }
                HyphenState::Second => {
                    if last_end == token.start && token_text == "-" {
                        state = HyphenState::Hyphen;
                    } else {
                        state = HyphenState::End;
                    }
                }
                HyphenState::End => {}
            }

            if state == HyphenState::End && hyphenation_start + 1 < i {
                let hyp_start = tokens[hyphenation_start].start;
                let hyp_end = tokens[i - 1].end;
                let snippet = &text[c2b[hyp_start]..c2b[hyp_end]];

                // Check conditions: must contain letters, or be repeated hyphens
                if self.letter_re.is_match(snippet) || self.only_hyphens_re.is_match(snippet) {
                    let covered: Vec<MatchSpan> = tokens[hyphenation_start..i].to_vec();
                    let normalized = self.normalize_word_with_hyphens(snippet);
                    result.push(CompoundToken {
                        span: MatchSpan::new(hyp_start, hyp_end),
                        token_spans: covered,
                        pattern_type: vec!["hyphenation".to_string()],
                        normalized,
                    });
                }
                state = HyphenState::None;
                hyphenation_start = i;
            }

            last_end = token.end;
        }

        result
    }

    /// Apply level 2 patterns (non-strict matching with token boundary constraints).
    fn apply_level2(
        &self,
        text: &str,
        tokens: &[MatchSpan],
        existing_compounds: &[CompoundToken],
        _c2b: &[usize],
        b2c: &[usize],
    ) -> Vec<CompoundToken> {
        let mut result: Vec<CompoundToken> = existing_compounds.to_vec();

        for pattern in &self.level2_patterns {
            let left_strict = pattern.left_strict.unwrap_or(true);
            let right_strict = pattern.right_strict.unwrap_or(true);

            for caps in pattern.regex.captures_iter(text) {
                let m = caps.get(0).unwrap();
                let byte_start = m.start();
                let byte_end = m.end();

                // Extract group span from captures directly (no re-running regex)
                let (group_byte_start, group_byte_end) = if pattern.group == 0 {
                    (byte_start, byte_end)
                } else if let Some(grp) = caps.get(pattern.group) {
                    (grp.start(), grp.end())
                } else {
                    continue;
                };

                let char_start = b2c[group_byte_start];
                let char_end = b2c[group_byte_end];
                let hint_span = MatchSpan::new(char_start, char_end);

                let match_text = &text[group_byte_start..group_byte_end];
                if self.contains_disallowed(match_text) {
                    continue;
                }

                // Find covered tokens
                let covered_tokens: Vec<MatchSpan> = tokens
                    .iter()
                    .filter(|t| {
                        if !left_strict && right_strict {
                            hint_span.start <= t.end && t.end <= hint_span.end
                        } else if left_strict && !right_strict {
                            hint_span.start <= t.start && t.start <= hint_span.end
                        } else {
                            hint_span.start <= t.start && t.end <= hint_span.end
                        }
                    })
                    .copied()
                    .collect();

                // Find covered existing compound tokens
                let covered_compounds: Vec<usize> = result
                    .iter()
                    .enumerate()
                    .filter(|(_, ct)| {
                        if !left_strict && right_strict {
                            hint_span.start <= ct.span.end && ct.span.end <= hint_span.end
                        } else if left_strict && !right_strict {
                            hint_span.start <= ct.span.start && ct.span.start <= hint_span.end
                        } else {
                            hint_span.start <= ct.span.start && ct.span.end <= hint_span.end
                        }
                    })
                    .map(|(i, _)| i)
                    .collect();

                // Remove regular tokens that are inside compound tokens
                let filtered_tokens: Vec<MatchSpan> = covered_tokens
                    .iter()
                    .filter(|t| {
                        !covered_compounds.iter().any(|&ci| {
                            result[ci].span.start <= t.start && t.end <= result[ci].span.end
                        })
                    })
                    .copied()
                    .collect();

                if covered_compounds.is_empty() && filtered_tokens.is_empty() {
                    continue;
                }

                // Check boundary constraints
                let leftmost_token = filtered_tokens.first().map(|t| t.start).unwrap_or(usize::MAX);
                let leftmost_compound = covered_compounds.iter().map(|&i| result[i].span.start).min().unwrap_or(usize::MAX);
                let leftmost = leftmost_token.min(leftmost_compound);

                let rightmost_token = filtered_tokens.last().map(|t| t.end).unwrap_or(0);
                let rightmost_compound = covered_compounds.iter().map(|&i| result[i].span.end).max().unwrap_or(0);
                let rightmost = rightmost_token.max(rightmost_compound);

                if left_strict && char_start != leftmost {
                    continue;
                }
                if right_strict && char_end != rightmost {
                    continue;
                }

                // Build merged token spans
                let mut all_base_spans: Vec<MatchSpan> = Vec::new();
                for &ci in &covered_compounds {
                    all_base_spans.extend_from_slice(&result[ci].token_spans);
                }
                for &t in &filtered_tokens {
                    if !all_base_spans.contains(&t) {
                        all_base_spans.push(t);
                    }
                }
                all_base_spans.sort_by(|a, b| a.start.cmp(&b.start).then(a.end.cmp(&b.end)));
                all_base_spans.dedup();

                if all_base_spans.is_empty() {
                    continue;
                }

                let new_span = MatchSpan::new(
                    all_base_spans.first().unwrap().start,
                    all_base_spans.last().unwrap().end,
                );

                let normalized = self.apply_normalization(
                    &pattern.normalization,
                    text,
                    group_byte_start,
                    group_byte_end,
                    &pattern.regex,
                );

                // Collect types from covered compounds + new pattern type
                let mut all_types: Vec<String> = Vec::new();
                for &ci in &covered_compounds {
                    all_types.extend(result[ci].pattern_type.clone());
                }
                all_types.push(pattern.pattern_type.clone());

                let new_ct = CompoundToken {
                    span: new_span,
                    token_spans: all_base_spans,
                    pattern_type: all_types,
                    normalized,
                };

                // Remove covered compounds (in reverse order to preserve indices)
                let mut to_remove: Vec<usize> = covered_compounds;
                to_remove.sort_unstable();
                to_remove.dedup();
                for &idx in to_remove.iter().rev() {
                    result.remove(idx);
                }

                // Insert new compound token at the right position
                let insert_pos = result
                    .iter()
                    .position(|ct| ct.span.start > new_ct.span.start)
                    .unwrap_or(result.len());
                result.insert(insert_pos, new_ct);
            }
        }

        result
    }

    /// Apply normalization to matched text.
    fn apply_normalization(
        &self,
        action: &NormalizationAction,
        text: &str,
        byte_start: usize,
        byte_end: usize,
        regex: &Regex,
    ) -> Option<String> {
        let full_match = &text[byte_start..byte_end];
        match action {
            NormalizationAction::None => None,
            NormalizationAction::StripWhitespace => {
                let result: String = full_match.chars().filter(|c| !c.is_whitespace()).collect();
                Some(result)
            }
            NormalizationAction::StripWhitespaceGroup(group) => {
                // Re-run with captures to get the specific group text
                if let Some(caps) = regex.captures(full_match) {
                    if let Some(grp) = caps.get(*group) {
                        let result: String = grp.as_str().chars().filter(|c| !c.is_whitespace()).collect();
                        return Some(result);
                    }
                }
                None
            }
            NormalizationAction::NumericWithPeriodNormalizer => {
                if self.month_year_re.is_match(full_match) || self.day_month_re.is_match(full_match) {
                    Some(full_match.to_string())
                } else {
                    let result: String = full_match.chars().filter(|&c| c != '.').collect();
                    Some(result)
                }
            }
            NormalizationAction::StripPeriods => {
                let result: String = full_match.replace('.', "");
                let result: String = result.chars().filter(|c| !c.is_whitespace()).collect();
                Some(result)
            }
            NormalizationAction::CompactPeriods => {
                // Remove spaces around periods: "a . b" -> "a.b"
                let result = self.compact_period_re1.replace_all(full_match, ".").to_string();
                let result = self.compact_period_re2.replace_all(&result, ".").to_string();
                Some(result)
            }
            NormalizationAction::CollapseWhitespace => {
                let result = self.collapse_ws_re.replace_all(full_match, " ").to_string();
                Some(result)
            }
        }
    }

    /// Check if the text contains any disallowed separator strings.
    fn contains_disallowed(&self, text: &str) -> bool {
        self.config.do_not_join_on_strings.iter().any(|s| text.contains(s.as_str()))
    }

    /// Attempt to normalize a hyphenated word.
    /// Returns None if no normalization needed.
    fn normalize_word_with_hyphens(&self, _word_text: &str) -> Option<String> {
        // The Python code calls MorphAnalyzedToken for normalization.
        // For now, return None (the ignored_words_with_hyphens.csv is empty anyway).
        None
    }
}

/// Check if a level 1 pattern type is allowed by the config.
fn is_pattern_allowed(pattern_type: &str, config: &CompoundTokenConfig) -> bool {
    let pt = pattern_type.to_lowercase();
    match pt.as_str() {
        "numeric" | "numeric_date" | "numeric_time" | "roman_numerals" => config.tag_numbers,
        "unit" => config.tag_units,
        "xml_tag" => config.tag_xml,
        "email" | "www_address" | "www_address_short" => config.tag_email_and_www,
        "emoticon" => config.tag_emoticons,
        "abbreviation" | "non_ending_abbreviation" => config.tag_abbreviations,
        "name_with_initial" => config.tag_initials,
        s if s.starts_with("negative:") => config.tag_initials,
        "hashtag" | "username_mention" => config.tag_hashtags_and_usernames,
        _ => true,
    }
}

/// Check if a level 2 pattern type is allowed by the config.
fn is_level2_pattern_allowed(pattern_type: &str, config: &CompoundTokenConfig) -> bool {
    let pt = pattern_type.to_lowercase();
    match pt.as_str() {
        "case_ending" => config.tag_case_endings,
        "sign" | "percentage" => config.tag_numbers,
        _ => true,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_compound_token_tagger_create() {
        let tagger = CompoundTokenTagger::estonian();
        assert!(!tagger.level1_patterns.is_empty());
    }

    #[test]
    fn test_detect_date() {
        let tagger = CompoundTokenTagger::estonian();
        let text = "Kuupäev on 02.02.2010 ja rohkem pole";
        let token_tagger = crate::tokens_tagger::TokensTagger::new();
        let tokens = token_tagger.tokenize(text);
        let compounds = tagger.detect(text, &tokens);
        let date_ct = compounds.iter().find(|ct| ct.pattern_type.contains(&"numeric_date".to_string()));
        assert!(date_ct.is_some(), "Expected to find a numeric_date compound token");
    }

    #[test]
    fn test_detect_hyphenation() {
        let tagger = CompoundTokenTagger::estonian();
        let text = "Vana-Tallinn on ilus";
        let token_tagger = crate::tokens_tagger::TokensTagger::new();
        let tokens = token_tagger.tokenize(text);
        let compounds = tagger.detect(text, &tokens);
        let hyp = compounds.iter().find(|ct| ct.pattern_type.contains(&"hyphenation".to_string()));
        assert!(hyp.is_some(), "Expected to find a hyphenation compound token");
    }
}
