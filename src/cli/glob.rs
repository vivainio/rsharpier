//! EditorConfig style globs, as used by .editorconfig sections and csharpierrc overrides

use regex::Regex;

pub struct GlobMatcher {
    regex: Regex,
}

impl GlobMatcher {
    pub fn is_match(&self, path: &str) -> bool {
        self.regex.is_match(&path.replace('\\', "/"))
    }
}

/// Port of Globber.Create: globs without a slash match in any directory, a leading slash anchors
/// to the directory of the config file
pub fn create(files: &str, directory: Option<&str>) -> GlobMatcher {
    let mut glob = match files.find('/') {
        None => format!("**/{files}"),
        Some(0) => files[1..].to_string(),
        Some(_) => files.to_string(),
    };
    if let Some(dir) = directory {
        let mut dir = dir.replace('\\', "/");
        if !dir.ends_with('/') {
            dir.push('/');
        }
        glob = format!("{dir}{glob}");
    }
    GlobMatcher {
        regex: Regex::new(&format!("^{}$", translate(&glob))).unwrap_or_else(|_| Regex::new("^$a").unwrap()),
    }
}

fn translate(glob: &str) -> String {
    let chars: Vec<char> = glob.chars().collect();
    let mut out = String::new();
    let mut i = 0;
    let mut brace_depth = 0;
    while i < chars.len() {
        let c = chars[i];
        match c {
            '*' => {
                if chars.get(i + 1) == Some(&'*') {
                    // `**/` matches zero or more directories
                    if chars.get(i + 2) == Some(&'/') {
                        out.push_str("(?:.*/)?");
                        i += 3;
                        continue;
                    }
                    out.push_str(".*");
                    i += 2;
                    continue;
                }
                out.push_str("[^/]*");
            }
            '?' => out.push_str("[^/]"),
            '[' => {
                // a character class if there is a closing bracket
                if let Some(end) = chars[i + 1..].iter().position(|c| *c == ']') {
                    let inner: String = chars[i + 1..i + 1 + end].iter().collect();
                    let (neg, inner) = match inner.strip_prefix(['!', '^']) {
                        Some(rest) => (true, rest.to_string()),
                        None => (false, inner),
                    };
                    out.push('[');
                    if neg {
                        out.push('^');
                    }
                    for ch in inner.chars() {
                        if matches!(ch, '\\' | '[' | ']' | '^') {
                            out.push('\\');
                        }
                        out.push(ch);
                    }
                    out.push(']');
                    i += end + 2;
                    continue;
                }
                out.push_str("\\[");
            }
            '{' => {
                // {a,b} alternatives or {1..3} numeric ranges
                if let Some(end) = find_closing_brace(&chars, i) {
                    let inner: String = chars[i + 1..end].iter().collect();
                    if let Some((a, b)) = inner.split_once("..") {
                        if let (Ok(a), Ok(b)) = (a.parse::<i64>(), b.parse::<i64>()) {
                            let (lo, hi) = (a.min(b), a.max(b));
                            let alts: Vec<String> = (lo..=hi.min(lo + 1000)).map(|n| n.to_string()).collect();
                            out.push_str(&format!("(?:{})", alts.join("|")));
                            i = end + 1;
                            continue;
                        }
                    }
                    let parts = split_top_level(&inner);
                    let translated: Vec<String> = parts.iter().map(|p| translate(p)).collect();
                    out.push_str(&format!("(?:{})", translated.join("|")));
                    i = end + 1;
                    continue;
                }
                out.push_str("\\{");
            }
            '\\' => {
                if let Some(n) = chars.get(i + 1) {
                    out.push_str(&regex::escape(&n.to_string()));
                    i += 2;
                    continue;
                }
                out.push_str("\\\\");
            }
            _ => out.push_str(&regex::escape(&c.to_string())),
        }
        i += 1;
        let _ = &mut brace_depth;
    }
    out
}

fn find_closing_brace(chars: &[char], start: usize) -> Option<usize> {
    let mut depth = 0;
    for (i, c) in chars.iter().enumerate().skip(start) {
        match c {
            '{' => depth += 1,
            '}' => {
                depth -= 1;
                if depth == 0 {
                    return Some(i);
                }
            }
            _ => {}
        }
    }
    None
}

fn split_top_level(s: &str) -> Vec<String> {
    let mut parts = Vec::new();
    let mut depth = 0;
    let mut cur = String::new();
    for c in s.chars() {
        match c {
            '{' => {
                depth += 1;
                cur.push(c)
            }
            '}' => {
                depth -= 1;
                cur.push(c)
            }
            ',' if depth == 0 => parts.push(std::mem::take(&mut cur)),
            _ => cur.push(c),
        }
    }
    parts.push(cur);
    parts
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn matches_like_editorconfig() {
        assert!(create("*.cs", Some("/repo")).is_match("/repo/src/a.cs"));
        assert!(!create("*.cs", Some("/repo")).is_match("/repo/src/a.xml"));
        assert!(create("{*.cs,*.csx}", None).is_match("/x/y/a.csx"));
        assert!(create("/src/*.cs", Some("/repo")).is_match("/repo/src/a.cs"));
        assert!(!create("/src/*.cs", Some("/repo")).is_match("/repo/src/deep/a.cs"));
        assert!(create("src/**/*.cs", Some("/repo")).is_match("/repo/src/a/b/a.cs"));
        assert!(create("src/**/*.cs", Some("/repo")).is_match("/repo/src/a.cs"));
    }
}
