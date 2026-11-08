use regex::Regex;
use std::sync::LazyLock;

static JUNK_TITLE_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^\d+[-_]\d+[-_]\d+$").unwrap());

static NOISE_BRACKETS_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)\s*[\(\[（【][^)\]）】]*(?:remaster|soundtrack|from\s+[\x22“]|album\s+version|single\s+version|deluxe|anniversary|explicit)[^)\]）】]*[\)\]）】]").unwrap()
});

static NOISE_DASH_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?i)\s+-\s+(?:\d{4}\s+)?remaster(?:ed)?.*$").unwrap());

static ARTIST_ALIAS_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^([^\(（]+?)[\(（]([^\)）]+)[\)）]$").unwrap());

pub fn is_junk_stream(title: &str, artist: &str) -> bool {
    let t = title.trim();
    let a = artist.trim();
    if t.is_empty() || a.is_empty() {
        return true;
    }
    JUNK_TITLE_RE.is_match(t)
}

pub fn clean_title(title: &str) -> String {
    let t = title.trim();
    if t.is_empty() {
        return String::new();
    }
    let s = NOISE_BRACKETS_RE.replace_all(t, "");
    let s = NOISE_DASH_RE.replace_all(&s, "");
    let res = s.trim();
    if res.is_empty() {
        t.to_string()
    } else {
        res.to_string()
    }
}

pub fn clean_artist(artist: &str) -> String {
    let t = artist.trim();
    if t.is_empty() {
        return String::new();
    }
    let first = t.split('/').next().unwrap_or("").trim();
    if let Some(caps) = ARTIST_ALIAS_RE.captures(first) {
        if let Some(m) = caps.get(1) {
            return m.as_str().trim().to_string();
        }
    }
    first.to_string()
}

pub fn artist_variants(artist: &str) -> Vec<String> {
    let mut out = Vec::new();
    for part in artist.split('/') {
        let part = part.trim();
        if part.is_empty() {
            continue;
        }
        if let Some(caps) = ARTIST_ALIAS_RE.captures(part) {
            if let Some(m1) = caps.get(1) {
                let s1 = m1.as_str().trim();
                if !s1.is_empty() {
                    out.push(s1.to_string());
                }
            }
            if let Some(m2) = caps.get(2) {
                let s2 = m2.as_str().trim();
                if !s2.is_empty() {
                    out.push(s2.to_string());
                }
            }
        } else {
            out.push(part.to_string());
        }
    }
    out
}

pub fn norm(s: &str) -> String {
    if s.is_empty() {
        return String::new();
    }
    let replaced = s.to_lowercase().replace('&', "and");
    let mut result = String::with_capacity(replaced.len());
    for c in replaced.chars() {
        // [0-9a-z\u3040-\u30ff\u3400-\u4dbf\u4e00-\u9fff\uac00-\ud7af]
        let u = c as u32;
        let is_allowed = c.is_ascii_digit()
            || c.is_ascii_lowercase()
            || (0x3040..=0x30ff).contains(&u)
            || (0x3400..=0x4dbf).contains(&u)
            || (0x4e00..=0x9fff).contains(&u)
            || (0xac00..=0xd7af).contains(&u);
        if is_allowed {
            result.push(c);
        }
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_norm() {
        assert_eq!(norm("Hello & World! 123"), "helloandworld123");
        assert_eq!(norm("桜流し (Live)"), "桜流しlive");
        assert_eq!(norm("没人比我更爱我 · 徐薇"), "没人比我更爱我徐薇");
    }

    #[test]
    fn test_clean_artist() {
        assert_eq!(clean_artist("冯沁苑(买辣椒也用券) / 某某"), "冯沁苑");
        assert_eq!(clean_artist("宇多田ヒカル"), "宇多田ヒカル");
    }

    #[test]
    fn test_artist_variants() {
        let v = artist_variants("冯沁苑(买辣椒也用券) / 某某");
        assert_eq!(v, vec!["冯沁苑", "买辣椒也用券", "某某"]);
    }

    #[test]
    fn test_clean_title() {
        assert_eq!(
            clean_title("Hotel California (2013 Remaster)"),
            "Hotel California"
        );
        assert_eq!(clean_title("Track - 2020 Remastered"), "Track");
    }

    #[test]
    fn test_is_junk_stream() {
        assert!(is_junk_stream("", "artist"));
        assert!(is_junk_stream("title", ""));
        assert!(is_junk_stream("1234-5678-9012", "artist"));
        assert!(!is_junk_stream("Valid Title", "Valid Artist"));
    }
}
