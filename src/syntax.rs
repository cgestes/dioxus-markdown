//! Optional syntax highlighting for fenced code blocks.
//!
//! When the `syntax-highlighting` Cargo feature is enabled, highlighting is
//! powered by `two-face`'s pure-Rust `syntect` grammar bundle (`syntect-fancy`)
//! and mapped into a compact set of semantic [`HighlightKind`] classes styled
//! with CSS variables in `theme.css`. When disabled (e.g. for slim WASM
//! builds), [`highlight_code`] is a zero-cost no-op.
use std::ops::Range;

/// Semantic syntax token category mapped to a `.tok-*` CSS class.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[repr(u8)]
pub enum HighlightKind {
    Plain = 0,
    Comment = 1,
    Keyword = 2,
    String = 3,
    Number = 4,
    Type = 5,
    Function = 6,
    Constant = 7,
    Attribute = 8,
    Operator = 9,
    Tag = 10,
}

impl HighlightKind {
    /// Converts a raw discriminant back to a [`HighlightKind`], defaulting
    /// unknown values to [`HighlightKind::Plain`].
    pub const fn from_u8(v: u8) -> Self {
        match v {
            1 => Self::Comment,
            2 => Self::Keyword,
            3 => Self::String,
            4 => Self::Number,
            5 => Self::Type,
            6 => Self::Function,
            7 => Self::Constant,
            8 => Self::Attribute,
            9 => Self::Operator,
            10 => Self::Tag,
            _ => Self::Plain,
        }
    }

    /// CSS class for this token category (empty string for [`HighlightKind::Plain`]).
    pub const fn css_class(self) -> &'static str {
        match self {
            Self::Plain => "",
            Self::Comment => "tok-comment",
            Self::Keyword => "tok-keyword",
            Self::String => "tok-string",
            Self::Number => "tok-number",
            Self::Type => "tok-type",
            Self::Function => "tok-function",
            Self::Constant => "tok-constant",
            Self::Attribute => "tok-attribute",
            Self::Operator => "tok-operator",
            Self::Tag => "tok-tag",
        }
    }
}

/// A highlighted UTF-8 byte range inside a code block body.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HighlightSpan {
    pub range: Range<usize>,
    pub kind: HighlightKind,
}

/// A selectable code-block syntax language for language pickers.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LanguageOption {
    /// Canonical Markdown fence info token (`""` for plain text).
    pub token: String,
    /// Human-readable display label (e.g. `"Rust"`, `"TypeScript"`, `"Plain text"`).
    pub name: String,
    /// Lowercase aliases/extensions matched during search.
    pub aliases: Vec<String>,
}

const CURATED_LANGUAGES: &[(&str, &str, &[&str])] = &[
    ("", "Plain text", &["plain", "text", "txt", "none"]),
    ("rust", "Rust", &["rs"]),
    ("ts", "TypeScript", &["typescript"]),
    ("tsx", "TSX", &["typescriptreact"]),
    ("js", "JavaScript", &["javascript", "mjs", "cjs"]),
    ("jsx", "JSX", &["javascriptreact"]),
    ("python", "Python", &["py", "python3"]),
    ("bash", "Bash", &["sh", "shell", "zsh", "console"]),
    ("json", "JSON", &["jsonc"]),
    ("toml", "TOML", &[]),
    ("yaml", "YAML", &["yml"]),
    ("sql", "SQL", &["postgres", "sqlite"]),
    ("html", "HTML", &["htm"]),
    ("css", "CSS", &[]),
    ("markdown", "Markdown", &["md"]),
    ("go", "Go", &["golang"]),
    ("c", "C", &["h"]),
    ("cpp", "C++", &["c++", "cxx", "cc", "hpp"]),
    ("java", "Java", &[]),
    ("swift", "Swift", &[]),
    ("kotlin", "Kotlin", &["kt"]),
    ("zig", "Zig", &[]),
    ("ruby", "Ruby", &["rb"]),
    ("php", "PHP", &[]),
    ("csharp", "C#", &["cs"]),
    ("dockerfile", "Dockerfile", &["docker"]),
    ("diff", "Diff", &["patch"]),
    ("xml", "XML", &["svg"]),
    ("lua", "Lua", &[]),
    ("nix", "Nix", &[]),
    ("graphql", "GraphQL", &["gql"]),
    ("makefile", "Makefile", &["make"]),
];

/// Returns all available syntax options (popular languages first, followed by
/// the remaining grammars bundled in `two-face` when `syntax-highlighting` is enabled).
pub fn available_languages() -> &'static [LanguageOption] {
    static ALL: std::sync::OnceLock<Vec<LanguageOption>> = std::sync::OnceLock::new();
    ALL.get_or_init(|| {
        #[allow(unused_mut)]
        let mut out: Vec<LanguageOption> = CURATED_LANGUAGES
            .iter()
            .map(|(token, name, aliases)| LanguageOption {
                token: (*token).to_owned(),
                name: (*name).to_owned(),
                aliases: aliases.iter().map(|a| (*a).to_owned()).collect(),
            })
            .collect();
        #[cfg(feature = "syntax-highlighting")]
        imp::append_extra_languages(&mut out);
        out
    })
}

/// Filter [`available_languages`] by `query`, ranking exact and prefix matches
/// ahead of substring matches.
pub fn filter_languages(query: &str) -> Vec<&'static LanguageOption> {
    let q = query.trim().to_ascii_lowercase();
    let all = available_languages();
    if q.is_empty() {
        return all.iter().collect();
    }
    let mut ranked: Vec<(u8, usize, &'static LanguageOption)> = Vec::new();
    for (idx, opt) in all.iter().enumerate() {
        let name = opt.name.to_ascii_lowercase();
        let token = opt.token.to_ascii_lowercase();
        let rank = if token == q || name == q || opt.aliases.iter().any(|a| a == &q) {
            Some(0)
        } else if token.starts_with(&q)
            || name.starts_with(&q)
            || opt.aliases.iter().any(|a| a.starts_with(&q))
        {
            Some(1)
        } else if name.contains(&q)
            || token.contains(&q)
            || opt.aliases.iter().any(|a| a.contains(&q))
        {
            Some(2)
        } else {
            None
        };
        if let Some(r) = rank {
            ranked.push((r, idx, opt));
        }
    }
    ranked.sort_by_key(|(r, idx, _)| (*r, *idx));
    ranked.into_iter().map(|(_, _, opt)| opt).collect()
}

/// Human-friendly display label for a Markdown fence language token.
pub fn display_language(lang: &str) -> String {
    let trimmed = lang.trim();
    if trimmed.is_empty() {
        return "Plain text".to_owned();
    }
    let lower = trimmed.to_ascii_lowercase();
    for opt in available_languages() {
        if opt.token == lower
            || opt.name.eq_ignore_ascii_case(trimmed)
            || opt.aliases.iter().any(|a| a == &lower)
        {
            return opt.name.clone();
        }
    }
    trimmed.to_owned()
}

/// Highlight `code` according to fenced language identifier `lang`.
///
/// Returned spans are sorted, non-overlapping, never cross line breaks, and
/// contain only non-[`HighlightKind::Plain`] ranges in UTF-8 byte offsets
/// relative to `code`.
///
/// Returns an empty `Vec` when `lang` is empty or unrecognized, `code` is
/// empty, or the `syntax-highlighting` feature is not enabled.
#[cfg(not(feature = "syntax-highlighting"))]
pub fn highlight_code(_lang: &str, _code: &str) -> Vec<HighlightSpan> {
    Vec::new()
}

#[cfg(feature = "syntax-highlighting")]
pub fn highlight_code(lang: &str, code: &str) -> Vec<HighlightSpan> {
    imp::highlight_code(lang, code)
}

#[cfg(feature = "syntax-highlighting")]
mod imp {
    use super::{HighlightKind, HighlightSpan};
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};
    use std::sync::{Mutex, OnceLock};
    use two_face::re_exports::syntect::parsing::{
        ParseState, Scope, ScopeStack, SyntaxReference, SyntaxSet,
    };
    use two_face::re_exports::syntect::util::LinesWithEndings;

    const CACHE_CAPACITY: usize = 64;
    const MAX_CACHED_BYTES: usize = 64 * 1024;

    const SCOPE_RULES: &[(&str, HighlightKind)] = &[
        ("comment", HighlightKind::Comment),
        ("constant.character.escape", HighlightKind::Constant),
        ("constant.numeric", HighlightKind::Number),
        ("support.type.property-name", HighlightKind::Attribute),
        ("entity.other.attribute-name", HighlightKind::Attribute),
        ("meta.annotation", HighlightKind::Attribute),
        ("meta.attribute", HighlightKind::Attribute),
        ("string", HighlightKind::String),
        ("keyword.operator", HighlightKind::Operator),
        ("punctuation.accessor", HighlightKind::Operator),
        ("keyword", HighlightKind::Keyword),
        ("storage", HighlightKind::Keyword),
        ("entity.name.function", HighlightKind::Function),
        ("support.function", HighlightKind::Function),
        ("variable.function", HighlightKind::Function),
        ("entity.name.tag", HighlightKind::Tag),
        ("entity.name.type", HighlightKind::Type),
        ("entity.name.struct", HighlightKind::Type),
        ("entity.name.enum", HighlightKind::Type),
        ("entity.name.trait", HighlightKind::Type),
        ("entity.name.class", HighlightKind::Type),
        ("entity.name.interface", HighlightKind::Type),
        ("entity.other.inherited-class", HighlightKind::Type),
        ("support.type", HighlightKind::Type),
        ("support.class", HighlightKind::Type),
        ("constant", HighlightKind::Constant),
        ("variable.language", HighlightKind::Constant),
        ("markup.inserted", HighlightKind::String),
        ("markup.deleted", HighlightKind::Tag),
        ("markup.changed", HighlightKind::Keyword),
        ("markup.heading", HighlightKind::Keyword),
    ];

    struct ScopeMatchers {
        rules: Vec<(Scope, HighlightKind)>,
        meta_mapping_key: Scope,
    }

    impl ScopeMatchers {
        fn new() -> Self {
            let scope = |name: &str| Scope::new(name).expect("valid static TextMate scope");
            Self {
                rules: SCOPE_RULES
                    .iter()
                    .map(|&(name, kind)| (scope(name), kind))
                    .collect(),
                meta_mapping_key: scope("meta.mapping.key"),
            }
        }

        fn classify(&self, stack: &ScopeStack) -> HighlightKind {
            // Walk innermost-to-outermost so specific scopes (escapes, embedded
            // interpolation expressions, property keys) take precedence over
            // enclosing container scopes.
            for &scope in stack.scopes.iter().rev() {
                for &(rule, kind) in &self.rules {
                    if !rule.is_prefix_of(scope) {
                        continue;
                    }
                    // JSON/YAML/TOML mapping keys are often scoped as
                    // `meta.mapping.key string.quoted`; distinguish keys from values.
                    if kind == HighlightKind::String
                        && stack
                            .scopes
                            .iter()
                            .any(|&s| self.meta_mapping_key.is_prefix_of(s))
                    {
                        return HighlightKind::Attribute;
                    }
                    return kind;
                }
            }
            HighlightKind::Plain
        }
    }

    fn syntax_set() -> &'static SyntaxSet {
        static SET: OnceLock<SyntaxSet> = OnceLock::new();
        SET.get_or_init(two_face::syntax::extra_newlines)
    }

    pub(super) fn append_extra_languages(out: &mut Vec<super::LanguageOption>) {
        use std::collections::HashSet;
        let mut seen: HashSet<String> = HashSet::new();
        for opt in out.iter() {
            seen.insert(opt.name.to_ascii_lowercase());
            if !opt.token.is_empty() {
                seen.insert(opt.token.to_ascii_lowercase());
            }
            for a in &opt.aliases {
                seen.insert(a.to_ascii_lowercase());
            }
        }
        let mut extras: Vec<super::LanguageOption> = Vec::new();
        for syn in syntax_set().syntaxes() {
            if syn.hidden || syn.name.eq_ignore_ascii_case("plain text") {
                continue;
            }
            let lower_name = syn.name.to_ascii_lowercase();
            if seen.contains(&lower_name) {
                continue;
            }
            let exts: Vec<String> = syn
                .file_extensions
                .iter()
                .map(|e| e.to_ascii_lowercase())
                .filter(|e| !e.is_empty())
                .collect();
            let token = exts
                .first()
                .cloned()
                .unwrap_or_else(|| lower_name.replace(' ', "-"));
            if seen.contains(&token) {
                continue;
            }
            seen.insert(lower_name);
            seen.insert(token.clone());
            extras.push(super::LanguageOption {
                token,
                name: syn.name.clone(),
                aliases: exts,
            });
        }
        extras.sort_by(|a, b| a.name.to_ascii_lowercase().cmp(&b.name.to_ascii_lowercase()));
        out.extend(extras);
    }

    fn matchers() -> &'static ScopeMatchers {
        static MATCHERS: OnceLock<ScopeMatchers> = OnceLock::new();
        MATCHERS.get_or_init(ScopeMatchers::new)
    }

    fn find_syntax<'a>(set: &'a SyntaxSet, lang: &str) -> Option<&'a SyntaxReference> {
        let token = lang.trim();
        if token.is_empty() {
            return None;
        }
        let lower = token.to_ascii_lowercase();
        let canonical = match lower.as_str() {
            "plaintext" | "plain" | "text" | "txt" => return None,
            "rs" | "rust" => "rs",
            "ts" | "typescript" => "ts",
            "tsx" => "tsx",
            "js" | "javascript" | "mjs" | "cjs" => "js",
            "jsx" => "jsx",
            "py" | "python" | "python3" => "py",
            "sh" | "bash" | "zsh" | "shell" | "console" => "sh",
            "yml" | "yaml" => "yaml",
            "md" | "markdown" => "md",
            "jsonc" => "json",
            "c++" | "cpp" | "cxx" => "cpp",
            "cs" | "csharp" => "cs",
            "dockerfile" => "Dockerfile",
            _ => token,
        };
        set.find_syntax_by_token(canonical)
            .or_else(|| set.find_syntax_by_token(token))
    }

    struct CacheEntry {
        hash: u64,
        lang: String,
        code: String,
        spans: Vec<HighlightSpan>,
    }

    fn cache() -> &'static Mutex<Vec<CacheEntry>> {
        static CACHE: OnceLock<Mutex<Vec<CacheEntry>>> = OnceLock::new();
        CACHE.get_or_init(|| Mutex::new(Vec::with_capacity(CACHE_CAPACITY)))
    }

    fn hash_key(lang: &str, code: &str) -> u64 {
        let mut h = DefaultHasher::new();
        lang.hash(&mut h);
        0xffu8.hash(&mut h);
        code.hash(&mut h);
        h.finish()
    }

    pub fn highlight_code(lang: &str, code: &str) -> Vec<HighlightSpan> {
        let lang = lang.trim();
        if lang.is_empty() || code.is_empty() {
            return Vec::new();
        }
        let key_lang = lang.to_ascii_lowercase();
        let can_cache = code.len() <= MAX_CACHED_BYTES;
        let hash = if can_cache {
            hash_key(&key_lang, code)
        } else {
            0
        };
        if can_cache && let Ok(mut guard) = cache().lock() {
            if let Some(pos) = guard
                .iter()
                .position(|e| e.hash == hash && e.lang == key_lang && e.code == code)
            {
                let entry = guard.remove(pos);
                let result = entry.spans.clone();
                guard.push(entry);
                return result;
            }
        }

        let set = syntax_set();
        let Some(syntax) = find_syntax(set, lang) else {
            return Vec::new();
        };
        let matchers = matchers();
        let mut state = ParseState::new(syntax);
        let mut stack = ScopeStack::new();
        let mut spans: Vec<HighlightSpan> = Vec::new();
        let mut line_offset = 0usize;

        for line in LinesWithEndings::from(code) {
            let line_content_len = line.trim_end_matches(['\r', '\n']).len();
            if let Ok(ops) = state.parse_line(line, set) {
                let mut prev = 0usize;
                for (col, op) in ops {
                    let end = col.min(line_content_len);
                    if end > prev {
                        let kind = matchers.classify(&stack);
                        push_span(&mut spans, line_offset + prev..line_offset + end, kind);
                    }
                    let _ = stack.apply(&op);
                    prev = col.min(line_content_len);
                }
                if prev < line_content_len {
                    let kind = matchers.classify(&stack);
                    push_span(
                        &mut spans,
                        line_offset + prev..line_offset + line_content_len,
                        kind,
                    );
                }
            }
            line_offset += line.len();
        }

        if can_cache && let Ok(mut guard) = cache().lock() {
            if guard.len() >= CACHE_CAPACITY {
                guard.remove(0);
            }
            guard.push(CacheEntry {
                hash,
                lang: key_lang,
                code: code.to_owned(),
                spans: spans.clone(),
            });
        }

        spans
    }

    fn push_span(spans: &mut Vec<HighlightSpan>, range: std::ops::Range<usize>, kind: HighlightKind) {
        if kind == HighlightKind::Plain || range.is_empty() {
            return;
        }
        if let Some(last) = spans.last_mut()
            && last.kind == kind
            && last.range.end == range.start
        {
            last.range.end = range.end;
        } else {
            spans.push(HighlightSpan { range, kind });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn highlight_kind_round_trips_and_css_classes_are_stable() {
        for v in 0..=10u8 {
            let kind = HighlightKind::from_u8(v);
            assert_eq!(kind as u8, v);
            if v == 0 {
                assert_eq!(kind.css_class(), "");
            } else {
                assert!(kind.css_class().starts_with("tok-"));
            }
        }
        assert_eq!(HighlightKind::from_u8(99), HighlightKind::Plain);
    }

    #[test]
    fn unknown_or_empty_language_returns_empty() {
        assert!(highlight_code("", "fn main() {}").is_empty());
        assert!(highlight_code("plaintext", "fn main() {}").is_empty());
        assert!(highlight_code("nonexistent-lang-xyz", "fn main() {}").is_empty());
        assert!(highlight_code("rust", "").is_empty());
    }

    #[test]
    fn language_catalog_filters_and_labels_cleanly() {
        assert_eq!(display_language(""), "Plain text");
        assert_eq!(display_language("rs"), "Rust");
        assert_eq!(display_language("ts"), "TypeScript");
        assert_eq!(display_language("custom-dsl"), "custom-dsl");
        let py = filter_languages("py");
        assert_eq!(py.first().map(|o| o.token.as_str()), Some("python"));
        let rs = filter_languages("rs");
        assert_eq!(rs.first().map(|o| o.token.as_str()), Some("rust"));
        assert_eq!(
            available_languages().first().map(|o| o.name.as_str()),
            Some("Plain text")
        );
    }

    #[cfg(feature = "syntax-highlighting")]
    #[test]
    fn highlights_rust_typescript_and_toml_with_valid_byte_ranges() {
        let rust = "// greet\nfn main() {\n    let msg = \"hi\";\n    let n = 42;\n}";
        let spans = highlight_code("rust", rust);
        assert!(!spans.is_empty());
        for w in spans.windows(2) {
            assert!(w[0].range.end <= w[1].range.start);
        }
        assert!(spans.iter().any(|s| &rust[s.range.clone()] == "// greet" && s.kind == HighlightKind::Comment));
        assert!(spans.iter().any(|s| &rust[s.range.clone()] == "fn" && s.kind == HighlightKind::Keyword));
        assert!(spans.iter().any(|s| &rust[s.range.clone()] == "main" && s.kind == HighlightKind::Function));
        assert!(spans.iter().any(|s| rust[s.range.clone()].contains("hi") && s.kind == HighlightKind::String));
        assert!(spans.iter().any(|s| &rust[s.range.clone()] == "42" && s.kind == HighlightKind::Number));

        // Verify languages from two-face's extra pack (TypeScript + TOML) work too.
        let ts = "interface User { name: string }\nconst x = 10;";
        let ts_spans = highlight_code("ts", ts);
        assert!(ts_spans.iter().any(|s| &ts[s.range.clone()] == "interface" && s.kind == HighlightKind::Keyword));
        assert!(ts_spans.iter().any(|s| &ts[s.range.clone()] == "10" && s.kind == HighlightKind::Number));

        let toml = "[package]\nname = \"margin\"\n";
        let toml_spans = highlight_code("toml", toml);
        assert!(toml_spans.iter().any(|s| toml[s.range.clone()].contains("margin") && s.kind == HighlightKind::String));
    }

    #[cfg(not(feature = "syntax-highlighting"))]
    #[test]
    fn feature_off_returns_no_spans() {
        assert!(highlight_code("rust", "fn main() {}").is_empty());
    }
}
