#[derive(Clone)]
pub enum SortBy {
    LevenshteinDistance,
    Ascending,
    None,
}

#[derive(Clone)]
pub enum Matcher {
    Prefix,
    Fuzzy,
}

#[derive(Clone)]
pub struct CompletionOptions {
    pub case_sensitive: bool,
    pub positional: bool,
    pub sort_by: SortBy,
    pub matcher: Matcher,
}

impl CompletionOptions {
    pub fn new(case_sensitive: bool, positional: bool, sort_by: SortBy, matcher: Matcher) -> Self {
        Self {
            case_sensitive,
            positional,
            sort_by,
            matcher,
        }
    }
}

impl Default for CompletionOptions {
    fn default() -> Self {
        Self {
            case_sensitive: true,
            positional: true,
            sort_by: SortBy::Ascending,
            matcher: Matcher::Fuzzy,
        }
    }
}
