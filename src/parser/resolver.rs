/// Fuzzy target resolver: exact > prefix > substring matching.
/// Returns matched entities or disambiguation list.

#[derive(Debug, Clone, PartialEq)]
pub enum ResolveResult {
    /// Exactly one match found
    Found(usize),
    /// Multiple matches — need disambiguation
    Ambiguous(Vec<(usize, String)>),
    /// No match
    NotFound,
}

/// Match a target string against a list of (id, name) candidates.
/// Priority: exact > prefix > substring (all case-insensitive).
pub fn resolve_target(target: &str, candidates: &[(usize, &str)]) -> ResolveResult {
    let target_lower = target.to_lowercase();

    // Phase 1: exact matches
    let exact: Vec<(usize, String)> = candidates.iter()
        .filter(|(_, name)| name.to_lowercase() == target_lower)
        .map(|(id, name)| (*id, name.to_string()))
        .collect();

    if exact.len() == 1 {
        return ResolveResult::Found(exact[0].0);
    }
    if exact.len() > 1 {
        return ResolveResult::Ambiguous(exact);
    }

    // Phase 2: prefix matches
    let prefix: Vec<(usize, String)> = candidates.iter()
        .filter(|(_, name)| name.to_lowercase().starts_with(&target_lower))
        .map(|(id, name)| (*id, name.to_string()))
        .collect();

    if prefix.len() == 1 {
        return ResolveResult::Found(prefix[0].0);
    }
    if prefix.len() > 1 {
        return ResolveResult::Ambiguous(prefix);
    }

    // Phase 3: substring matches
    let substring: Vec<(usize, String)> = candidates.iter()
        .filter(|(_, name)| name.to_lowercase().contains(&target_lower))
        .map(|(id, name)| (*id, name.to_string()))
        .collect();

    if substring.len() == 1 {
        return ResolveResult::Found(substring[0].0);
    }
    if substring.len() > 1 {
        return ResolveResult::Ambiguous(substring);
    }

    ResolveResult::NotFound
}

/// Format an ambiguous result into output lines.
pub fn format_disambiguation(matches: &[(usize, String)]) -> Vec<String> {
    let mut lines = vec!["Which do you mean?".to_string()];
    for (i, (_, name)) in matches.iter().enumerate() {
        lines.push(format!("  {}. {}", i + 1, name));
    }
    lines
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_exact_match() {
        let candidates = vec![(1, "Magnus the Keeper"), (2, "Magda the Bold")];
        assert_eq!(resolve_target("magnus the keeper", &candidates), ResolveResult::Found(1));
    }

    #[test]
    fn test_prefix_match() {
        let candidates = vec![(1, "Magnus the Keeper"), (2, "Kael the Bold")];
        assert_eq!(resolve_target("mag", &candidates), ResolveResult::Found(1));
    }

    #[test]
    fn test_substring_match() {
        let candidates = vec![(1, "Torn Map"), (2, "Iron Shield")];
        assert_eq!(resolve_target("map", &candidates), ResolveResult::Found(1));
    }

    #[test]
    fn test_ambiguous_prefix() {
        let candidates = vec![(1, "Magnus the Keeper"), (2, "Magda the Bold")];
        let result = resolve_target("mag", &candidates);
        match result {
            ResolveResult::Ambiguous(matches) => {
                assert_eq!(matches.len(), 2);
            }
            other => panic!("Expected Ambiguous, got {:?}", other),
        }
    }

    #[test]
    fn test_not_found() {
        let candidates = vec![(1, "Magnus the Keeper")];
        assert_eq!(resolve_target("nobody", &candidates), ResolveResult::NotFound);
    }

    #[test]
    fn test_exact_beats_prefix() {
        // "map" is an exact match for "Map", even though "Torn Map" has it as substring
        let candidates = vec![(1, "Map"), (2, "Torn Map")];
        assert_eq!(resolve_target("map", &candidates), ResolveResult::Found(1));
    }

    #[test]
    fn test_exact_beats_substring() {
        // "shield" exactly matches "Shield", even though "Iron Shield" also contains it
        let candidates = vec![(1, "Shield"), (2, "Iron Shield")];
        assert_eq!(resolve_target("shield", &candidates), ResolveResult::Found(1));
    }

    #[test]
    fn test_disambiguation_format() {
        let matches = vec![(1, "Magnus the Keeper".to_string()), (2, "Magda the Bold".to_string())];
        let lines = format_disambiguation(&matches);
        assert_eq!(lines[0], "Which do you mean?");
        assert!(lines[1].contains("1."));
        assert!(lines[1].contains("Magnus the Keeper"));
        assert!(lines[2].contains("2."));
        assert!(lines[2].contains("Magda the Bold"));
    }

    #[test]
    fn test_case_insensitive() {
        let candidates = vec![(1, "Torn Map")];
        assert_eq!(resolve_target("TORN MAP", &candidates), ResolveResult::Found(1));
    }
}
