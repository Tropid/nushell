use fuzzy_matcher::{skim::SkimMatcherV2, FuzzyMatcher as _};

pub trait TextMatcher {
    fn matches(&self, haystack: &str, needle: &str) -> Option<MatchScore>;
}

#[derive(PartialEq, Eq)]
pub struct MatchScore(pub i32);

pub struct FuzzyMatcher {
    matcher: SkimMatcherV2,
}

impl FuzzyMatcher {
    pub fn new() -> Self {
        Self {
            matcher: SkimMatcherV2::default(),
        }
    }
}

impl TextMatcher for FuzzyMatcher {
    fn matches(&self, haystack: &str, needle: &str) -> Option<MatchScore> {
        self.matcher
            .fuzzy_indices(haystack, needle)
            .map(|(score, _)| MatchScore(score as i32))
    }
}
