use std::borrow::Cow;
use std::collections::{HashMap, HashSet};

use estnltk_core::byte_to_char_map;
use estnltk_core::{resolve_conflicts, MatchEntry};
#[cfg(test)]
use estnltk_core::ConflictStrategy;
use estnltk_core::{
    assemble_tag_result, build_rule_annotation, check_unique_patterns, compute_rule_map,
    has_missing_attributes, AnnotationValue, MatchSpan, TaggerError, TagResult,
    TaggerConfig,
};
#[cfg(test)]
use estnltk_core::CommonConfig;

/// Common interface for tagger rules (regex, substring, span).
///
/// Allows shared functions (`build_rule_annotation`, `compute_rule_map`, etc.)
/// to operate on any rule type without knowing its concrete type.
///
/// Re-exported here because `ExtractionRule` lives in this crate (it depends on
/// `resharp::Regex` and `regex::Regex`), and it needs to implement this trait.
use estnltk_core::TaggerRule;

/// A compiled extraction rule.
/// Maps to EstNLTK's `StaticExtractionRule`.
pub struct ExtractionRule {
    pub pattern_str: String,
    pub compiled: resharp::Regex,
    /// Anchored `regex::Regex` for capture group extraction (only when `group > 0`).
    /// Compiled as `^(?:<pattern>)$` so it matches exactly the substring that
    /// resharp matched, eliminating leftmost-first vs leftmost-longest divergence.
    pub capture_re: Option<regex::Regex>,
    pub attributes: HashMap<String, AnnotationValue>,
    pub group: u32,
    pub priority: i32,
}

impl std::fmt::Debug for ExtractionRule {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ExtractionRule")
            .field("pattern_str", &self.pattern_str)
            .field("group", &self.group)
            .field("has_capture_re", &self.capture_re.is_some())
            .field("attributes", &self.attributes)
            .field("priority", &self.priority)
            .finish()
    }
}

impl TaggerRule for ExtractionRule {
    fn pattern_str(&self) -> &str {
        &self.pattern_str
    }
    fn attributes(&self) -> &HashMap<String, AnnotationValue> {
        &self.attributes
    }
    fn group(&self) -> u32 {
        self.group
    }
    fn priority(&self) -> i32 {
        self.priority
    }
}

/// The core regex tagger — Rust equivalent of EstNLTK's `RegexTagger`.
pub struct RegexTagger {
    pub rules: Vec<ExtractionRule>,
    pub config: TaggerConfig,
}

impl RegexTagger {
    /// Create a new tagger, validating configuration.
    pub fn new(rules: Vec<ExtractionRule>, config: TaggerConfig) -> Result<Self, TaggerError> {
        // Enforce unique patterns if configured (EstNLTK Ruleset semantics).
        if config.common.unique_patterns {
            let patterns: Vec<&str> = rules.iter().map(|r| r.pattern_str.as_str()).collect();
            check_unique_patterns(&patterns, config.lowercase_text)?;
        }

        Ok(Self { rules, config })
    }

    /// Run the full tagging pipeline on a text string.
    pub fn tag(&self, text: &str) -> TagResult {
        let raw_text: Cow<str> = if self.config.lowercase_text {
            Cow::Owned(text.to_lowercase())
        } else {
            Cow::Borrowed(text)
        };

        // Step 1: Extract all matches with byte→char conversion.
        let mut all_matches = if self.config.overlapped {
            self.extract_matches_overlapping(&raw_text)
        } else {
            self.extract_matches(&raw_text)
        };

        // Step 2: Sort canonically by (start, end).
        all_matches.sort_by_key(|&(span, _)| (span.start, span.end));

        // Step 3: Apply conflict resolution.
        let resolved = resolve_conflicts(
            self.config.common.conflict_strategy,
            &all_matches,
            |rule_idx| (self.rules[rule_idx].group as i32, self.rules[rule_idx].priority),
        );

        // Step 4: Build TagResult.
        self.build_result(&resolved, &raw_text)
    }

    /// Extract raw matches from all rules, converting byte→char offsets.
    ///
    /// For rules with `group > 0`, a two-pass approach narrows each resharp
    /// match to the requested capture group using an anchored `regex::Regex`:
    ///
    /// 1. resharp finds the full match (group 0) at byte offsets `[start, end]`
    /// 2. The anchored regex `^(?:<pattern>)$` runs on the substring `text[start..end]`
    /// 3. The anchoring forces the `regex` crate to cover the entire substring,
    ///    eliminating leftmost-first vs leftmost-longest divergence
    /// 4. The requested capture group's byte offsets are extracted and adjusted
    fn extract_matches(&self, text: &str) -> Vec<MatchEntry> {
        let b2c = byte_to_char_map(text);
        let text_bytes = text.as_bytes();
        let mut matches = Vec::new();

        for (rule_idx, rule) in self.rules.iter().enumerate() {
            let found = match rule.compiled.find_all(text_bytes) {
                Ok(v) => v,
                Err(_) => continue,
            };
            for m in found {
                let (byte_start, byte_end) = if rule.group == 0 {
                    (m.start, m.end)
                } else {
                    // Two-pass: narrow to capture group.
                    let capture_re = rule.capture_re.as_ref().unwrap();
                    let substring = match text.get(m.start..m.end) {
                        Some(s) => s,
                        None => continue,
                    };
                    if let Some(caps) = capture_re.captures(substring) {
                        if let Some(group_match) = caps.get(rule.group as usize) {
                            (m.start + group_match.start(), m.start + group_match.end())
                        } else {
                            continue; // Group didn't participate in this match
                        }
                    } else {
                        continue; // Anchored pattern didn't match substring
                    }
                };

                let char_start = b2c[byte_start];
                let char_end = b2c[byte_end];
                // Skip zero-length matches.
                if char_start == char_end {
                    continue;
                }
                matches.push((MatchSpan::new(char_start, char_end), rule_idx));
            }
        }

        matches
    }

    /// Extract overlapping matches from all rules.
    ///
    /// For each rule, repeatedly calls `find_all` on progressively advancing
    /// sub-slices of the input.  After collecting all non-overlapping matches
    /// from a given start position, the search restarts from `min_new_start + 1`
    /// (the next UTF-8 character boundary after the earliest newly discovered
    /// match) to find matches that overlap with previously found ones.
    ///
    /// Mirrors Python's `regex.finditer(pattern, text, overlapped=True)`.
    fn extract_matches_overlapping(&self, text: &str) -> Vec<MatchEntry> {
        let b2c = byte_to_char_map(text);
        let text_bytes = text.as_bytes();
        let mut matches = Vec::new();

        for (rule_idx, rule) in self.rules.iter().enumerate() {
            let mut seen: HashSet<(usize, usize)> = HashSet::new();
            let mut search_pos: usize = 0;

            while search_pos < text_bytes.len() {
                let sub = &text_bytes[search_pos..];
                let found = match rule.compiled.find_all(sub) {
                    Ok(v) => v,
                    Err(_) => break,
                };
                if found.is_empty() {
                    break;
                }

                let mut any_new = false;
                let mut min_new_start = usize::MAX;

                for m in &found {
                    let abs_start = m.start + search_pos;
                    let abs_end = m.end + search_pos;

                    // Narrow to capture group if needed.
                    let (byte_start, byte_end) = if rule.group == 0 {
                        (abs_start, abs_end)
                    } else {
                        let capture_re = rule.capture_re.as_ref().unwrap();
                        let substring = match text.get(abs_start..abs_end) {
                            Some(s) => s,
                            None => continue,
                        };
                        if let Some(caps) = capture_re.captures(substring) {
                            if let Some(group_match) = caps.get(rule.group as usize) {
                                (
                                    abs_start + group_match.start(),
                                    abs_start + group_match.end(),
                                )
                            } else {
                                continue;
                            }
                        } else {
                            continue;
                        }
                    };

                    if !seen.insert((byte_start, byte_end)) {
                        continue; // Already recorded this exact span for this rule.
                    }
                    any_new = true;
                    min_new_start = min_new_start.min(abs_start);

                    let char_start = b2c[byte_start];
                    let char_end = b2c[byte_end];
                    if char_start == char_end {
                        continue;
                    }
                    matches.push((MatchSpan::new(char_start, char_end), rule_idx));
                }

                if !any_new {
                    break;
                }

                // Advance to the next UTF-8 character boundary after the
                // earliest new match's start.
                search_pos = min_new_start + 1;
                while search_pos < text_bytes.len()
                    && !text.is_char_boundary(search_pos)
                {
                    search_pos += 1;
                }
            }
        }

        matches
    }

    /// Build the final TagResult from resolved matches.
    ///
    /// `text` is the (possibly lowercased) text that was matched against,
    /// used to extract matched substrings when `match_attribute` is set.
    fn build_result(&self, resolved: &[MatchEntry], text: &str) -> TagResult {
        // Build char→byte map only when match_attribute is set (O(n) once,
        // then O(1) per match instead of O(start+len) per match).
        let c2b = self
            .config
            .match_attribute
            .as_ref()
            .map(|_| estnltk_core::char_to_byte_map(text));

        let entries = resolved.iter().map(|&(match_span, rule_idx)| {
            let mut annotation = build_rule_annotation(
                &self.rules[rule_idx],
                &self.config.common.output_attributes,
                self.config.common.group_attribute.as_deref(),
                self.config.common.priority_attribute.as_deref(),
                self.config.common.pattern_attribute.as_deref(),
            );
            if let Some(ref attr_name) = self.config.match_attribute {
                let c2b = c2b.as_ref().unwrap();
                let matched_text = &text[c2b[match_span.start]..c2b[match_span.end]];
                annotation.insert(
                    attr_name.clone(),
                    AnnotationValue::Str(matched_text.to_string()),
                );
            }
            (match_span, annotation)
        });
        assemble_tag_result(
            entries,
            &self.config.common.output_layer,
            &self.config.common.output_attributes,
            self.config.common.ambiguous_output_layer,
        )
    }

    /// Check if rules have inconsistent attribute sets.
    ///
    /// Returns `true` if some rules don't define the same set of attributes.
    /// Maps to EstNLTK's `AmbiguousRuleset.missing_attributes` property.
    pub fn missing_attributes(&self) -> bool {
        let attrs: Vec<&HashMap<String, AnnotationValue>> =
            self.rules.iter().map(|r| &r.attributes).collect();
        has_missing_attributes(&attrs)
    }

    /// Return a map of pattern strings to their rule indices.
    ///
    /// Maps to EstNLTK's `Ruleset.rule_map` / `AmbiguousRuleset.rule_map` property.
    /// Groups rules by their pattern string, so patterns shared by multiple rules
    /// (ambiguous rules) map to multiple indices.
    pub fn rule_map(&self) -> HashMap<String, Vec<usize>> {
        compute_rule_map(&self.rules, self.config.lowercase_text)
    }
}

/// Convenience: build an ExtractionRule from components.
///
/// When `group > 0`, compiles an additional anchored `regex::Regex` pattern
/// (`^(?:<pattern>)$`) for capture group extraction. This is used in a two-pass
/// approach: resharp finds the full match, then the anchored regex extracts the
/// requested capture group from the matched substring.
pub fn make_rule(
    pattern: &str,
    attributes: HashMap<String, AnnotationValue>,
    group: u32,
    priority: i32,
) -> Result<ExtractionRule, TaggerError> {
    let compiled = resharp::Regex::new(pattern)
        .map_err(|e| TaggerError::InvalidRegex(format!(
            "Regex compile error for '{}': {}", pattern, e
        )))?;

    let capture_re = if group > 0 {
        let anchored = format!("^(?:{})$", pattern);
        let re = regex::Regex::new(&anchored).map_err(|e| {
            TaggerError::InvalidRegex(format!(
                "Capture group extraction failed for pattern '{}': {}. \
                 Patterns using resharp-only syntax (intersection, complement) \
                 cannot use capture groups (group > 0).",
                pattern, e
            ))
        })?;
        // Validate that the requested group exists in the pattern.
        // captures_len() returns group count including group 0.
        if group as usize >= re.captures_len() {
            return Err(TaggerError::Config(format!(
                "Rule requests group={} but pattern '{}' only has {} capture group(s) (0..{}).",
                group,
                pattern,
                re.captures_len() - 1,
                re.captures_len() - 1
            )));
        }
        Some(re)
    } else {
        None
    };

    Ok(ExtractionRule {
        pattern_str: pattern.to_string(),
        compiled,
        capture_re,
        attributes,
        group,
        priority,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn default_config() -> TaggerConfig {
        TaggerConfig {
            common: CommonConfig {
                output_layer: "test".to_string(),
                output_attributes: vec![],
                conflict_strategy: ConflictStrategy::KeepAll,
                group_attribute: None,
                priority_attribute: None,
                pattern_attribute: None,
                ambiguous_output_layer: true,
                unique_patterns: false,
            },
            lowercase_text: false,
            overlapped: false,
            match_attribute: None,
        }
    }

    #[test]
    fn test_simple_match() {
        let rule = make_rule("hello", HashMap::new(), 0, 0).unwrap();
        let tagger = RegexTagger::new(vec![rule], default_config()).unwrap();
        let result = tagger.tag("say hello world");
        assert_eq!(result.spans.len(), 1);
        assert_eq!(result.spans[0].span, MatchSpan::new(4, 9));
    }

    #[test]
    fn test_no_match() {
        let rule = make_rule("xyz", HashMap::new(), 0, 0).unwrap();
        let tagger = RegexTagger::new(vec![rule], default_config()).unwrap();
        let result = tagger.tag("hello world");
        assert_eq!(result.spans.len(), 0);
    }

    #[test]
    fn test_multiple_matches() {
        let rule = make_rule("ab", HashMap::new(), 0, 0).unwrap();
        let tagger = RegexTagger::new(vec![rule], default_config()).unwrap();
        let result = tagger.tag("ab cd ab");
        assert_eq!(result.spans.len(), 2);
        assert_eq!(result.spans[0].span, MatchSpan::new(0, 2));
        assert_eq!(result.spans[1].span, MatchSpan::new(6, 8));
    }

    #[test]
    fn test_estonian_multibyte() {
        // "öö" in "Tüüpiline öökülma näide"
        let rule = make_rule("öö", HashMap::new(), 0, 0).unwrap();
        let tagger = RegexTagger::new(vec![rule], default_config()).unwrap();
        let result = tagger.tag("Tüüpiline öökülma näide");
        assert_eq!(result.spans.len(), 1);
        // char offsets: T(0) ü(1) ü(2) p(3) i(4) l(5) i(6) n(7) e(8) (9)
        //               ö(10) ö(11) k(12) ü(13) l(14) m(15) a(16) (17)
        //               n(18) ä(19) i(20) d(21) e(22)
        assert_eq!(result.spans[0].span, MatchSpan::new(10, 12));
    }

    #[test]
    fn test_lowercase_flag() {
        let rule = make_rule("hello", HashMap::new(), 0, 0).unwrap();
        let mut cfg = default_config();
        cfg.lowercase_text = true;
        let tagger = RegexTagger::new(vec![rule], cfg).unwrap();
        let result = tagger.tag("HELLO world");
        assert_eq!(result.spans.len(), 1);
        assert_eq!(result.spans[0].span, MatchSpan::new(0, 5));
    }

    // ── Capture group tests ────────────────────────────────────────────

    #[test]
    fn test_capture_group_basic() {
        // Pattern: (Mr\.\s+)(\w+) — group 2 extracts the name.
        // Text: "Hello Mr. Smith there"
        //         0123456789...
        // resharp matches "Mr. Smith" (chars 6..15)
        // group 2 = "Smith" (chars 10..15)
        let rule = make_rule(r"(Mr\.\s+)(\w+)", HashMap::new(), 2, 0).unwrap();
        let tagger = RegexTagger::new(vec![rule], default_config()).unwrap();
        let result = tagger.tag("Hello Mr. Smith there");
        assert_eq!(result.spans.len(), 1);
        assert_eq!(result.spans[0].span, MatchSpan::new(10, 15));
    }

    #[test]
    fn test_capture_group_1() {
        // Same pattern, but extract group 1 ("Mr. ").
        let rule = make_rule(r"(Mr\.\s+)(\w+)", HashMap::new(), 1, 0).unwrap();
        let tagger = RegexTagger::new(vec![rule], default_config()).unwrap();
        let result = tagger.tag("Hello Mr. Smith there");
        assert_eq!(result.spans.len(), 1);
        // "Mr. " = chars 6..10
        assert_eq!(result.spans[0].span, MatchSpan::new(6, 10));
    }

    #[test]
    fn test_capture_group_multibyte() {
        // Estonian text: extract word after "Hr. " (Mr. in Estonian).
        // "Tere Hr. Tamm ja teised"
        //  T(0) e(1) r(2) e(3) ' '(4) H(5) r(6) .(7) ' '(8) T(9) a(10) m(11) m(12) ' '(13) ...
        let rule = make_rule(r"(Hr\.\s+)(\w+)", HashMap::new(), 2, 0).unwrap();
        let tagger = RegexTagger::new(vec![rule], default_config()).unwrap();
        let result = tagger.tag("Tere Hr. Tamm ja teised");
        assert_eq!(result.spans.len(), 1);
        assert_eq!(result.spans[0].span, MatchSpan::new(9, 13)); // "Tamm"
    }

    #[test]
    fn test_capture_group_multibyte_estonian_chars() {
        // Extract the accented word from a pattern.
        // "aasta: 2024, koht: Põltsamaa, linn"
        // Pattern captures the place name after "koht: "
        let rule = make_rule(r"(koht:\s+)(\w+)", HashMap::new(), 2, 0).unwrap();
        let tagger = RegexTagger::new(vec![rule], default_config()).unwrap();
        let result = tagger.tag("aasta: 2024, koht: Põltsamaa, linn");
        assert_eq!(result.spans.len(), 1);
        // "koht: Põltsamaa" is the full match.
        // "Põltsamaa" is group 2.
        // Count: a(0) a(1) s(2) t(3) a(4) :(5) ' '(6) 2(7) 0(8) 2(9) 4(10)
        //        ,(11) ' '(12) k(13) o(14) h(15) t(16) :(17) ' '(18)
        //        P(19) õ(20) l(21) t(22) s(23) a(24) m(25) a(26) a(27) ,(28)
        assert_eq!(result.spans[0].span, MatchSpan::new(19, 28)); // "Põltsamaa"
    }

    #[test]
    fn test_capture_group_multiple_matches() {
        // Multiple matches, each narrowed to group 1.
        let rule = make_rule(r"(\d+) EUR", HashMap::new(), 1, 0).unwrap();
        let tagger = RegexTagger::new(vec![rule], default_config()).unwrap();
        let result = tagger.tag("Hind: 100 EUR ja 250 EUR");
        assert_eq!(result.spans.len(), 2);
        assert_eq!(result.spans[0].span, MatchSpan::new(6, 9)); // "100"
        assert_eq!(result.spans[1].span, MatchSpan::new(17, 20)); // "250"
    }

    #[test]
    fn test_capture_group_with_attributes() {
        // Attributes still propagate correctly with capture groups.
        let mut attrs = HashMap::new();
        attrs.insert(
            "type".to_string(),
            AnnotationValue::Str("amount".to_string()),
        );
        let rule = make_rule(r"(\d+) EUR", attrs, 1, 0).unwrap();
        let mut cfg = default_config();
        cfg.common.output_attributes = vec!["type".to_string()];
        let tagger = RegexTagger::new(vec![rule], cfg).unwrap();
        let result = tagger.tag("Hind: 100 EUR");
        assert_eq!(result.spans.len(), 1);
        assert_eq!(result.spans[0].span, MatchSpan::new(6, 9));
        assert_eq!(
            result.spans[0].annotations[0].get("type"),
            Some(&AnnotationValue::Str("amount".to_string()))
        );
    }

    #[test]
    fn test_capture_group_mixed_rules() {
        // Mix group=0 and group>0 rules in the same tagger.
        let r1 = make_rule(r"(\d+) EUR", HashMap::new(), 1, 0).unwrap(); // group 1 → digits
        let r2 = make_rule(r"USD", HashMap::new(), 0, 0).unwrap(); // group 0 → full match
        let tagger = RegexTagger::new(vec![r1, r2], default_config()).unwrap();
        let result = tagger.tag("100 EUR and USD");
        assert_eq!(result.spans.len(), 2);
        assert_eq!(result.spans[0].span, MatchSpan::new(0, 3)); // "100" (narrowed)
        assert_eq!(result.spans[1].span, MatchSpan::new(12, 15)); // "USD" (full match)
    }

    #[test]
    fn test_capture_group_zero_length_skipped() {
        // If the captured group is zero-length, it should be skipped.
        // Pattern: (a*)(b+) on "bbb" — group 1 matches "" (zero-length), group 2 matches "bbb".
        let rule = make_rule(r"(a*)(b+)", HashMap::new(), 1, 0).unwrap();
        let tagger = RegexTagger::new(vec![rule], default_config()).unwrap();
        let result = tagger.tag("bbb");
        // group 1 = "" (zero-length), should be skipped.
        assert_eq!(result.spans.len(), 0);
    }

    #[test]
    fn test_capture_group_invalid_group_index() {
        // Pattern has 2 groups, requesting group 3 should fail at construction.
        let result = make_rule(r"(a)(b)", HashMap::new(), 3, 0);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("only has 2 capture group(s)"));
    }

    #[test]
    fn test_capture_group_no_groups_in_pattern() {
        // Pattern has no capture groups, requesting group 1 should fail.
        let result = make_rule(r"hello", HashMap::new(), 1, 0);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("only has 0 capture group(s)"));
    }

    #[test]
    fn test_capture_group_with_conflict_resolution() {
        // Two group>0 rules with overlapping narrowed spans.
        let r1 = make_rule(r"(Mr\.\s+)(\w+)", HashMap::new(), 2, 0).unwrap();
        let r2 = make_rule(r"\w+", HashMap::new(), 0, 0).unwrap();
        let mut cfg = default_config();
        cfg.common.conflict_strategy = ConflictStrategy::KeepMaximal;
        let tagger = RegexTagger::new(vec![r1, r2], cfg).unwrap();
        let result = tagger.tag("Mr. Smith");
        // r1 narrowed to "Smith" (5,9), r2 matches "Mr" (no, resharp gives leftmost-longest: first word is "Mr")
        // Actually \w+ doesn't match "." or " ", so matches are: "Mr" and "Smith"
        // r1 full match "Mr. Smith" narrowed to group 2 "Smith"
        // So we have spans: "Smith"(4,9) from r1, "Mr"(0,2) from r2, "Smith"(4,9) from r2
        // After KEEP_MAXIMAL: "Smith" subsumes nothing, "Mr" subsumes nothing — all kept
        assert!(result.spans.len() >= 2);
    }

    // ── End capture group tests ──────────────────────────────────────

    #[test]
    fn test_attributes_propagated() {
        let mut attrs = HashMap::new();
        attrs.insert("type".to_string(), AnnotationValue::Str("number".to_string()));
        let rule = make_rule("[0-9]+", attrs, 0, 0).unwrap();
        let mut cfg = default_config();
        cfg.common.output_attributes = vec!["type".to_string()];
        let tagger = RegexTagger::new(vec![rule], cfg).unwrap();
        let result = tagger.tag("abc 123 def");
        assert_eq!(result.spans.len(), 1);
        assert_eq!(
            result.spans[0].annotations[0].get("type"),
            Some(&AnnotationValue::Str("number".to_string()))
        );
    }

    #[test]
    fn test_muna_ja_kana_keep_all() {
        // Mirrors test_custom_conflict_resolver.py regex test
        let r1 = make_rule("m..a.ja", HashMap::new(), 0, 0).unwrap();
        let r2 = make_rule("ja", HashMap::new(), 0, 0).unwrap();
        let r3 = make_rule("ja.k..a", HashMap::new(), 0, 0).unwrap();

        let mut cfg = default_config();
        cfg.common.conflict_strategy = ConflictStrategy::KeepAll;
        cfg.lowercase_text = true;

        let tagger = RegexTagger::new(vec![r1, r2, r3], cfg).unwrap();
        let result = tagger.tag("Muna ja kana.");
        assert_eq!(result.spans.len(), 3);
        assert_eq!(result.spans[0].span, MatchSpan::new(0, 7));
        assert_eq!(result.spans[1].span, MatchSpan::new(5, 7));
        assert_eq!(result.spans[2].span, MatchSpan::new(5, 12));
    }

    #[test]
    fn test_muna_ja_kana_keep_maximal() {
        let r1 = make_rule("m..a.ja", HashMap::new(), 0, 0).unwrap();
        let r2 = make_rule("ja", HashMap::new(), 0, 0).unwrap();
        let r3 = make_rule("ja.k..a", HashMap::new(), 0, 0).unwrap();

        let mut cfg = default_config();
        cfg.common.conflict_strategy = ConflictStrategy::KeepMaximal;
        cfg.lowercase_text = true;

        let tagger = RegexTagger::new(vec![r1, r2, r3], cfg).unwrap();
        let result = tagger.tag("Muna ja kana.");
        assert_eq!(result.spans.len(), 2);
        assert_eq!(result.spans[0].span, MatchSpan::new(0, 7));
        assert_eq!(result.spans[1].span, MatchSpan::new(5, 12));
    }

    #[test]
    fn test_missing_attributes_false_consistent() {
        let mut a1 = HashMap::new();
        a1.insert("type".to_string(), AnnotationValue::Str("x".to_string()));
        let mut a2 = HashMap::new();
        a2.insert("type".to_string(), AnnotationValue::Str("y".to_string()));
        let r1 = make_rule("aaa", a1, 0, 0).unwrap();
        let r2 = make_rule("bbb", a2, 0, 0).unwrap();
        let tagger = RegexTagger::new(vec![r1, r2], default_config()).unwrap();
        assert!(!tagger.missing_attributes());
    }

    #[test]
    fn test_missing_attributes_true_inconsistent() {
        let mut a1 = HashMap::new();
        a1.insert("type".to_string(), AnnotationValue::Str("x".to_string()));
        a1.insert("color".to_string(), AnnotationValue::Str("red".to_string()));
        let mut a2 = HashMap::new();
        a2.insert("type".to_string(), AnnotationValue::Str("y".to_string()));
        let r1 = make_rule("aaa", a1, 0, 0).unwrap();
        let r2 = make_rule("bbb", a2, 0, 0).unwrap();
        let tagger = RegexTagger::new(vec![r1, r2], default_config()).unwrap();
        assert!(tagger.missing_attributes());
    }

    #[test]
    fn test_missing_attributes_single_rule() {
        let r1 = make_rule("aaa", HashMap::new(), 0, 0).unwrap();
        let tagger = RegexTagger::new(vec![r1], default_config()).unwrap();
        assert!(!tagger.missing_attributes());
    }

    #[test]
    fn test_missing_attributes_no_rules() {
        let tagger = RegexTagger::new(vec![], default_config()).unwrap();
        assert!(!tagger.missing_attributes());
    }

    #[test]
    fn test_normalize_annotations_fills_null() {
        // Rule 1 has {type, color}, rule 2 has {type} only.
        // output_attributes = ["type", "color"].
        // Rule 2's annotation should get color=Null.
        let mut a1 = HashMap::new();
        a1.insert("type".to_string(), AnnotationValue::Str("email".to_string()));
        a1.insert("color".to_string(), AnnotationValue::Str("red".to_string()));
        let mut a2 = HashMap::new();
        a2.insert("type".to_string(), AnnotationValue::Str("url".to_string()));

        let r1 = make_rule("aaa", a1, 0, 0).unwrap();
        let r2 = make_rule("bbb", a2, 0, 0).unwrap();

        let mut cfg = default_config();
        cfg.common.output_attributes = vec!["type".to_string(), "color".to_string()];

        let tagger = RegexTagger::new(vec![r1, r2], cfg).unwrap();
        let result = tagger.tag("aaa bbb");

        assert_eq!(result.spans.len(), 2);
        // First span: rule 1 has both attributes
        assert_eq!(
            result.spans[0].annotations[0].get("type"),
            Some(&AnnotationValue::Str("email".to_string()))
        );
        assert_eq!(
            result.spans[0].annotations[0].get("color"),
            Some(&AnnotationValue::Str("red".to_string()))
        );
        // Second span: rule 2 should have color=Null
        assert_eq!(
            result.spans[1].annotations[0].get("type"),
            Some(&AnnotationValue::Str("url".to_string()))
        );
        assert_eq!(
            result.spans[1].annotations[0].get("color"),
            Some(&AnnotationValue::Null)
        );
    }

    #[test]
    fn test_ambiguous_output_layer_false() {
        // Two rules match the same span — only first annotation kept.
        let mut a1 = HashMap::new();
        a1.insert("type".to_string(), AnnotationValue::Str("greeting".to_string()));
        let mut a2 = HashMap::new();
        a2.insert("type".to_string(), AnnotationValue::Str("word".to_string()));

        let r1 = make_rule("hello", a1, 0, 0).unwrap();
        let r2 = make_rule("hello", a2, 0, 0).unwrap();

        let mut cfg = default_config();
        cfg.common.output_attributes = vec!["type".to_string()];
        cfg.common.ambiguous_output_layer = false;

        let tagger = RegexTagger::new(vec![r1, r2], cfg).unwrap();
        let result = tagger.tag("hello");

        assert!(!result.ambiguous);
        assert_eq!(result.spans.len(), 1);
        // Only the first annotation is kept.
        assert_eq!(result.spans[0].annotations.len(), 1);
        assert_eq!(
            result.spans[0].annotations[0].get("type"),
            Some(&AnnotationValue::Str("greeting".to_string()))
        );
    }

    #[test]
    fn test_ambiguous_output_layer_true_default() {
        // Two rules match the same span — both annotations kept.
        let mut a1 = HashMap::new();
        a1.insert("type".to_string(), AnnotationValue::Str("greeting".to_string()));
        let mut a2 = HashMap::new();
        a2.insert("type".to_string(), AnnotationValue::Str("word".to_string()));

        let r1 = make_rule("hello", a1, 0, 0).unwrap();
        let r2 = make_rule("hello", a2, 0, 0).unwrap();

        let mut cfg = default_config();
        cfg.common.output_attributes = vec!["type".to_string()];

        let tagger = RegexTagger::new(vec![r1, r2], cfg).unwrap();
        let result = tagger.tag("hello");

        assert!(result.ambiguous);
        assert_eq!(result.spans.len(), 1);
        assert_eq!(result.spans[0].annotations.len(), 2);
    }

    #[test]
    fn test_unique_patterns_rejects_duplicate() {
        let r1 = make_rule("hello", HashMap::new(), 0, 0).unwrap();
        let r2 = make_rule("hello", HashMap::new(), 0, 0).unwrap();
        let mut cfg = default_config();
        cfg.common.unique_patterns = true;
        let result = RegexTagger::new(vec![r1, r2], cfg);
        assert!(result.is_err());
        assert!(result.err().unwrap().to_string().contains("Duplicate pattern"));
    }

    #[test]
    fn test_unique_patterns_allows_distinct() {
        let r1 = make_rule("hello", HashMap::new(), 0, 0).unwrap();
        let r2 = make_rule("world", HashMap::new(), 0, 0).unwrap();
        let mut cfg = default_config();
        cfg.common.unique_patterns = true;
        let result = RegexTagger::new(vec![r1, r2], cfg);
        assert!(result.is_ok());
    }

    #[test]
    fn test_unique_patterns_case_sensitive() {
        // "Hello" and "hello" are distinct when lowercase_text=false.
        let r1 = make_rule("Hello", HashMap::new(), 0, 0).unwrap();
        let r2 = make_rule("hello", HashMap::new(), 0, 0).unwrap();
        let mut cfg = default_config();
        cfg.common.unique_patterns = true;
        let result = RegexTagger::new(vec![r1, r2], cfg);
        assert!(result.is_ok());
    }

    #[test]
    fn test_unique_patterns_case_insensitive_duplicate() {
        // "Hello" and "hello" collapse to same key when lowercase_text=true.
        let r1 = make_rule("Hello", HashMap::new(), 0, 0).unwrap();
        let r2 = make_rule("hello", HashMap::new(), 0, 0).unwrap();
        let mut cfg = default_config();
        cfg.common.unique_patterns = true;
        cfg.lowercase_text = true;
        let result = RegexTagger::new(vec![r1, r2], cfg);
        assert!(result.is_err());
        assert!(result.err().unwrap().to_string().contains("Duplicate pattern"));
    }

    #[test]
    fn test_unique_patterns_false_allows_duplicates() {
        // Default behavior: duplicates allowed.
        let r1 = make_rule("hello", HashMap::new(), 0, 0).unwrap();
        let r2 = make_rule("hello", HashMap::new(), 0, 0).unwrap();
        let result = RegexTagger::new(vec![r1, r2], default_config());
        assert!(result.is_ok());
    }

    #[test]
    fn test_muna_ja_kana_keep_minimal() {
        let r1 = make_rule("m..a.ja", HashMap::new(), 0, 0).unwrap();
        let r2 = make_rule("ja", HashMap::new(), 0, 0).unwrap();
        let r3 = make_rule("ja.k..a", HashMap::new(), 0, 0).unwrap();

        let mut cfg = default_config();
        cfg.common.conflict_strategy = ConflictStrategy::KeepMinimal;
        cfg.lowercase_text = true;

        let tagger = RegexTagger::new(vec![r1, r2, r3], cfg).unwrap();
        let result = tagger.tag("Muna ja kana.");
        assert_eq!(result.spans.len(), 1);
        assert_eq!(result.spans[0].span, MatchSpan::new(5, 7));
    }

    // ── Overlapped matching tests ──────────────────────────────────────

    #[test]
    fn test_overlapped_aa_in_aaa() {
        // Pattern "aa" on "aaa" with overlapped=true should find 2 matches:
        // (0,2) and (1,3).
        let rule = make_rule("aa", HashMap::new(), 0, 0).unwrap();
        let mut cfg = default_config();
        cfg.overlapped = true;
        let tagger = RegexTagger::new(vec![rule], cfg).unwrap();
        let result = tagger.tag("aaa");
        assert_eq!(result.spans.len(), 2);
        assert_eq!(result.spans[0].span, MatchSpan::new(0, 2));
        assert_eq!(result.spans[1].span, MatchSpan::new(1, 3));
    }

    #[test]
    fn test_overlapped_false_aa_in_aaa() {
        // Without overlapped, "aa" on "aaa" finds only 1 match.
        let rule = make_rule("aa", HashMap::new(), 0, 0).unwrap();
        let tagger = RegexTagger::new(vec![rule], default_config()).unwrap();
        let result = tagger.tag("aaa");
        assert_eq!(result.spans.len(), 1);
    }

    #[test]
    fn test_overlapped_no_overlap() {
        // Non-overlapping matches should be identical with overlapped=true.
        let rule = make_rule("ab", HashMap::new(), 0, 0).unwrap();
        let mut cfg = default_config();
        cfg.overlapped = true;
        let tagger = RegexTagger::new(vec![rule], cfg).unwrap();
        let result = tagger.tag("ab cd ab");
        assert_eq!(result.spans.len(), 2);
        assert_eq!(result.spans[0].span, MatchSpan::new(0, 2));
        assert_eq!(result.spans[1].span, MatchSpan::new(6, 8));
    }

    #[test]
    fn test_overlapped_estonian_multibyte() {
        // Pattern "öö" on "öööö" (4 ö's) should find 3 overlapping matches.
        let rule = make_rule("öö", HashMap::new(), 0, 0).unwrap();
        let mut cfg = default_config();
        cfg.overlapped = true;
        let tagger = RegexTagger::new(vec![rule], cfg).unwrap();
        let result = tagger.tag("öööö");
        assert_eq!(result.spans.len(), 3);
        assert_eq!(result.spans[0].span, MatchSpan::new(0, 2));
        assert_eq!(result.spans[1].span, MatchSpan::new(1, 3));
        assert_eq!(result.spans[2].span, MatchSpan::new(2, 4));
    }

    #[test]
    fn test_overlapped_multiple_rules() {
        // Two rules, both with overlapping matches.
        let r1 = make_rule("aba", HashMap::new(), 0, 0).unwrap();
        let r2 = make_rule("bab", HashMap::new(), 0, 0).unwrap();
        let mut cfg = default_config();
        cfg.overlapped = true;
        let tagger = RegexTagger::new(vec![r1, r2], cfg).unwrap();
        let result = tagger.tag("abab");
        // r1 "aba" matches (0,3)
        // r2 "bab" matches (1,4)
        assert_eq!(result.spans.len(), 2);
        assert_eq!(result.spans[0].span, MatchSpan::new(0, 3));
        assert_eq!(result.spans[1].span, MatchSpan::new(1, 4));
    }

    #[test]
    fn test_overlapped_with_attributes() {
        let mut attrs = HashMap::new();
        attrs.insert("type".to_string(), AnnotationValue::Str("pair".to_string()));
        let rule = make_rule("aa", attrs, 0, 0).unwrap();
        let mut cfg = default_config();
        cfg.overlapped = true;
        cfg.common.output_attributes = vec!["type".to_string()];
        let tagger = RegexTagger::new(vec![rule], cfg).unwrap();
        let result = tagger.tag("aaa");
        assert_eq!(result.spans.len(), 2);
        for span in &result.spans {
            assert_eq!(
                span.annotations[0].get("type"),
                Some(&AnnotationValue::Str("pair".to_string()))
            );
        }
    }

    #[test]
    fn test_overlapped_with_capture_group() {
        // Overlapping with capture groups: (\d)\d on "123" finds
        // group 1 at (0,1) and (1,2).
        let rule = make_rule(r"(\d)\d", HashMap::new(), 1, 0).unwrap();
        let mut cfg = default_config();
        cfg.overlapped = true;
        let tagger = RegexTagger::new(vec![rule], cfg).unwrap();
        let result = tagger.tag("123");
        assert_eq!(result.spans.len(), 2);
        assert_eq!(result.spans[0].span, MatchSpan::new(0, 1)); // "1"
        assert_eq!(result.spans[1].span, MatchSpan::new(1, 2)); // "2"
    }

    #[test]
    fn test_overlapped_with_conflict_resolution() {
        // Overlapping matches + KEEP_MAXIMAL.
        let rule = make_rule("aa", HashMap::new(), 0, 0).unwrap();
        let mut cfg = default_config();
        cfg.overlapped = true;
        cfg.common.conflict_strategy = ConflictStrategy::KeepMaximal;
        let tagger = RegexTagger::new(vec![rule], cfg).unwrap();
        let result = tagger.tag("aaa");
        // Overlapped finds (0,2) and (1,3). Neither covers the other,
        // so KEEP_MAXIMAL keeps both.
        assert_eq!(result.spans.len(), 2);
    }

    #[test]
    fn test_overlapped_no_match() {
        let rule = make_rule("xyz", HashMap::new(), 0, 0).unwrap();
        let mut cfg = default_config();
        cfg.overlapped = true;
        let tagger = RegexTagger::new(vec![rule], cfg).unwrap();
        let result = tagger.tag("hello");
        assert_eq!(result.spans.len(), 0);
    }

    #[test]
    fn test_overlapped_single_char_pattern() {
        // Single-char pattern — overlapped should behave same as non-overlapped.
        let rule = make_rule("a", HashMap::new(), 0, 0).unwrap();
        let mut cfg = default_config();
        cfg.overlapped = true;
        let tagger = RegexTagger::new(vec![rule], cfg).unwrap();
        let result = tagger.tag("aaa");
        assert_eq!(result.spans.len(), 3);
    }

    // ── match_attribute tests ────────────────────────────────────────

    #[test]
    fn test_match_attribute_basic() {
        // Stores matched text under the configured attribute name.
        let rule = make_rule("[0-9]+", HashMap::new(), 0, 0).unwrap();
        let mut cfg = default_config();
        cfg.match_attribute = Some("match".to_string());
        let tagger = RegexTagger::new(vec![rule], cfg).unwrap();
        let result = tagger.tag("abc 123 def 456");
        assert_eq!(result.spans.len(), 2);
        assert_eq!(
            result.spans[0].annotations[0].get("match"),
            Some(&AnnotationValue::Str("123".to_string()))
        );
        assert_eq!(
            result.spans[1].annotations[0].get("match"),
            Some(&AnnotationValue::Str("456".to_string()))
        );
    }

    #[test]
    fn test_match_attribute_with_capture_group() {
        // When group > 0, match text should be the capture group text.
        let rule = make_rule(r"(\d+) EUR", HashMap::new(), 1, 0).unwrap();
        let mut cfg = default_config();
        cfg.match_attribute = Some("match".to_string());
        let tagger = RegexTagger::new(vec![rule], cfg).unwrap();
        let result = tagger.tag("Hind: 100 EUR ja 250 EUR");
        assert_eq!(result.spans.len(), 2);
        assert_eq!(
            result.spans[0].annotations[0].get("match"),
            Some(&AnnotationValue::Str("100".to_string()))
        );
        assert_eq!(
            result.spans[1].annotations[0].get("match"),
            Some(&AnnotationValue::Str("250".to_string()))
        );
    }

    #[test]
    fn test_match_attribute_estonian_multibyte() {
        // Matched text with Estonian characters is extracted correctly.
        let rule = make_rule(r"\w+maa", HashMap::new(), 0, 0).unwrap();
        let mut cfg = default_config();
        cfg.match_attribute = Some("matched".to_string());
        let tagger = RegexTagger::new(vec![rule], cfg).unwrap();
        let result = tagger.tag("Tere Põltsamaa ja Võrumaa");
        assert_eq!(result.spans.len(), 2);
        assert_eq!(
            result.spans[0].annotations[0].get("matched"),
            Some(&AnnotationValue::Str("Põltsamaa".to_string()))
        );
        assert_eq!(
            result.spans[1].annotations[0].get("matched"),
            Some(&AnnotationValue::Str("Võrumaa".to_string()))
        );
    }

    #[test]
    fn test_match_attribute_with_lowercase() {
        // When lowercase_text=true, match text comes from the lowercased input.
        let rule = make_rule("hello", HashMap::new(), 0, 0).unwrap();
        let mut cfg = default_config();
        cfg.lowercase_text = true;
        cfg.match_attribute = Some("match".to_string());
        let tagger = RegexTagger::new(vec![rule], cfg).unwrap();
        let result = tagger.tag("HELLO world");
        assert_eq!(result.spans.len(), 1);
        assert_eq!(
            result.spans[0].annotations[0].get("match"),
            Some(&AnnotationValue::Str("hello".to_string()))
        );
    }

    #[test]
    fn test_match_attribute_with_other_attributes() {
        // match_attribute coexists with static rule attributes.
        let mut attrs = HashMap::new();
        attrs.insert("type".to_string(), AnnotationValue::Str("email".to_string()));
        let rule = make_rule(r"\S+@\S+", attrs, 0, 0).unwrap();
        let mut cfg = default_config();
        cfg.common.output_attributes = vec!["type".to_string()];
        cfg.match_attribute = Some("match".to_string());
        let tagger = RegexTagger::new(vec![rule], cfg).unwrap();
        let result = tagger.tag("contact user@example.com today");
        assert_eq!(result.spans.len(), 1);
        assert_eq!(
            result.spans[0].annotations[0].get("type"),
            Some(&AnnotationValue::Str("email".to_string()))
        );
        assert_eq!(
            result.spans[0].annotations[0].get("match"),
            Some(&AnnotationValue::Str("user@example.com".to_string()))
        );
    }

    #[test]
    fn test_match_attribute_none_disabled() {
        // When match_attribute is None (default), no match text is stored.
        let rule = make_rule("hello", HashMap::new(), 0, 0).unwrap();
        let tagger = RegexTagger::new(vec![rule], default_config()).unwrap();
        let result = tagger.tag("hello");
        assert_eq!(result.spans.len(), 1);
        assert!(result.spans[0].annotations[0].get("match").is_none());
    }

    #[test]
    fn test_match_attribute_overlapped() {
        // Overlapping matches each store their own matched text.
        let rule = make_rule("aa", HashMap::new(), 0, 0).unwrap();
        let mut cfg = default_config();
        cfg.overlapped = true;
        cfg.match_attribute = Some("match".to_string());
        let tagger = RegexTagger::new(vec![rule], cfg).unwrap();
        let result = tagger.tag("aaa");
        assert_eq!(result.spans.len(), 2);
        assert_eq!(
            result.spans[0].annotations[0].get("match"),
            Some(&AnnotationValue::Str("aa".to_string()))
        );
        assert_eq!(
            result.spans[1].annotations[0].get("match"),
            Some(&AnnotationValue::Str("aa".to_string()))
        );
    }

    // ── rule_map tests ────────────────────────────────────────────────

    #[test]
    fn test_rule_map_distinct_patterns() {
        let r1 = make_rule("aaa", HashMap::new(), 0, 0).unwrap();
        let r2 = make_rule("bbb", HashMap::new(), 0, 0).unwrap();
        let tagger = RegexTagger::new(vec![r1, r2], default_config()).unwrap();
        let map = tagger.rule_map();
        assert_eq!(map.len(), 2);
        assert_eq!(map["aaa"], vec![0]);
        assert_eq!(map["bbb"], vec![1]);
    }

    #[test]
    fn test_rule_map_ambiguous_same_pattern() {
        // Two rules sharing the same pattern are grouped together.
        let mut a1 = HashMap::new();
        a1.insert("type".to_string(), AnnotationValue::Str("x".to_string()));
        let mut a2 = HashMap::new();
        a2.insert("type".to_string(), AnnotationValue::Str("y".to_string()));
        let r1 = make_rule("hello", a1, 0, 0).unwrap();
        let r2 = make_rule("hello", a2, 0, 0).unwrap();
        let tagger = RegexTagger::new(vec![r1, r2], default_config()).unwrap();
        let map = tagger.rule_map();
        assert_eq!(map.len(), 1);
        assert_eq!(map["hello"], vec![0, 1]);
    }

    #[test]
    fn test_rule_map_case_insensitive() {
        // With lowercase_text, patterns are grouped by lowercased key.
        let r1 = make_rule("Hello", HashMap::new(), 0, 0).unwrap();
        let r2 = make_rule("hello", HashMap::new(), 0, 0).unwrap();
        let mut cfg = default_config();
        cfg.lowercase_text = true;
        let tagger = RegexTagger::new(vec![r1, r2], cfg).unwrap();
        let map = tagger.rule_map();
        assert_eq!(map.len(), 1);
        assert_eq!(map["hello"], vec![0, 1]);
    }

    #[test]
    fn test_rule_map_case_sensitive() {
        // Without lowercase_text, "Hello" and "hello" are distinct.
        let r1 = make_rule("Hello", HashMap::new(), 0, 0).unwrap();
        let r2 = make_rule("hello", HashMap::new(), 0, 0).unwrap();
        let tagger = RegexTagger::new(vec![r1, r2], default_config()).unwrap();
        let map = tagger.rule_map();
        assert_eq!(map.len(), 2);
        assert_eq!(map["Hello"], vec![0]);
        assert_eq!(map["hello"], vec![1]);
    }

    #[test]
    fn test_rule_map_empty() {
        let tagger = RegexTagger::new(vec![], default_config()).unwrap();
        let map = tagger.rule_map();
        assert!(map.is_empty());
    }

    #[test]
    fn test_rule_map_three_rules_two_patterns() {
        let r1 = make_rule("aaa", HashMap::new(), 0, 0).unwrap();
        let r2 = make_rule("bbb", HashMap::new(), 0, 0).unwrap();
        let r3 = make_rule("aaa", HashMap::new(), 0, 1).unwrap();
        let tagger = RegexTagger::new(vec![r1, r2, r3], default_config()).unwrap();
        let map = tagger.rule_map();
        assert_eq!(map.len(), 2);
        assert_eq!(map["aaa"], vec![0, 2]);
        assert_eq!(map["bbb"], vec![1]);
    }
}
