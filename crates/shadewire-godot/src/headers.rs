//! Request headers scoped to the hosts they belong to: a document may name any host, so a game's CDN token must
//! not reach a third-party URL in a mod's document. Header values are secrets: no message here contains one.

use std::collections::BTreeMap;

/// `cdn.example.com`, `*.example.org` (any subdomain, not the domain itself), either with an optional `:port`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HostPattern {
    /// Lowercase; for a wildcard, the suffix after `*.`.
    host: String,
    wildcard: bool,
    port: Option<u16>,
}

impl HostPattern {
    pub fn parse(pattern: &str) -> Result<HostPattern, String> {
        let bad = || format!("host pattern \"{pattern}\": expected host, *.domain, optionally with :port");
        let lower = pattern.trim().to_ascii_lowercase();
        let (host, port) = if let Some(literal) = lower.strip_prefix('[') {
            let close = literal.find(']').ok_or_else(bad)?;
            let port = literal[close + 1..].strip_prefix(':');
            (literal[..close].to_string(), port.map(str::to_string))
        } else {
            match lower.rsplit_once(':') {
                Some((host, port)) if !host.contains(':') => (host.to_string(), Some(port.to_string())),
                _ => (lower.clone(), None),
            }
        };
        let port = port.map(|p| p.parse::<u16>().map_err(|_| bad())).transpose()?;
        let (host, wildcard) = match host.strip_prefix("*.") {
            Some(suffix) => (suffix.to_string(), true),
            None => (host, false),
        };
        let host = host.trim_end_matches('.').to_string();
        if host.is_empty() || host.contains(['*', '/', ' ']) {
            return Err(bad());
        }
        Ok(HostPattern { host, wildcard, port })
    }

    /// `host` is lowercase, as `Url` keeps it.
    pub fn matches(&self, host: &str, port: u16) -> bool {
        let host_ok = if self.wildcard {
            host.strip_suffix(self.host.as_str()).is_some_and(|sub| sub.len() > 1 && sub.ends_with('.'))
        } else {
            host == self.host
        };
        host_ok && self.port.is_none_or(|p| p == port)
    }

    /// Exact hosts beat wildcards, longer wildcards beat shorter ones, a port beats none.
    fn specificity(&self) -> (bool, usize, bool) {
        (!self.wildcard, self.host.len(), self.port.is_some())
    }
}

/// A `Name: value` line, checked: a token name and no line breaks (no header injection).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Header {
    pub name: String,
    pub value: String,
}

impl Header {
    /// Errors name the header, never its value.
    pub fn parse(line: &str) -> Result<Header, String> {
        let (name, value) = line.split_once(':').ok_or("a header must be \"Name: value\"")?;
        let name = name.trim();
        let token = |c: char| c.is_ascii_alphanumeric() || "!#$%&'*+-.^_`|~".contains(c);
        if name.is_empty() || !name.chars().all(token) {
            return Err(format!("header name \"{name}\" is not a valid token"));
        }
        let value = value.trim();
        if value.contains(['\r', '\n', '\0']) {
            return Err(format!("header {name}: the value contains a line break"));
        }
        Ok(Header { name: name.to_string(), value: value.to_string() })
    }

    pub fn line(&self) -> String {
        format!("{}: {}", self.name, self.value)
    }

    pub fn parse_all<'a>(lines: impl IntoIterator<Item = &'a str>) -> Result<Vec<Header>, String> {
        lines.into_iter().map(Header::parse).collect()
    }
}

/// The headers merged into one list, later ones replacing earlier ones by name (case-insensitively), in order.
pub fn merge(layers: impl IntoIterator<Item = Vec<Header>>) -> Vec<Header> {
    let mut by_name: BTreeMap<String, (usize, Header)> = BTreeMap::new();
    for (order, header) in layers.into_iter().flatten().enumerate() {
        let key = header.name.to_ascii_lowercase();
        let slot = by_name.get(&key).map_or(order, |(i, _)| *i);
        by_name.insert(key, (slot, header));
    }
    let mut headers: Vec<(usize, Header)> = by_name.into_values().collect();
    headers.sort_by_key(|(i, _)| *i);
    headers.into_iter().map(|(_, h)| h).collect()
}

/// `set_http_headers` rules: each pattern holds its headers, several patterns can apply.
#[derive(Clone, Debug, Default)]
pub struct HeaderRules {
    rules: Vec<(HostPattern, Vec<Header>)>,
}

impl HeaderRules {
    /// Replaces the headers of `pattern`; no headers removes the rule.
    pub fn set(&mut self, pattern: &str, headers: Vec<Header>) -> Result<(), String> {
        let pattern = HostPattern::parse(pattern)?;
        self.rules.retain(|(p, _)| *p != pattern);
        if !headers.is_empty() {
            self.rules.push((pattern, headers));
        }
        Ok(())
    }

    pub fn clear(&mut self) {
        self.rules.clear();
    }

    /// The headers of every matching pattern, the most specific winning a name clash.
    pub fn for_host(&self, host: &str, port: u16) -> Vec<Header> {
        let mut matching: Vec<&(HostPattern, Vec<Header>)> =
            self.rules.iter().filter(|(p, _)| p.matches(host, port)).collect();
        matching.sort_by_key(|(p, _)| p.specificity());
        merge(matching.into_iter().map(|(_, h)| h.clone()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn headers(lines: &[&str]) -> Vec<Header> {
        Header::parse_all(lines.iter().copied()).unwrap()
    }

    fn lines(headers: &[Header]) -> Vec<String> {
        headers.iter().map(Header::line).collect()
    }

    #[test]
    fn exact_hosts_match_themselves_only() {
        let p = HostPattern::parse("CDN.example.com").unwrap();
        assert!(p.matches("cdn.example.com", 443) && p.matches("cdn.example.com", 8080));
        assert!(!p.matches("evil-cdn.example.com", 443) && !p.matches("cdn.example.com.evil.org", 443));
        assert!(!p.matches("example.com", 443));
    }

    #[test]
    fn wildcards_match_subdomains_not_the_domain() {
        let p = HostPattern::parse("*.assets.example.org").unwrap();
        assert!(p.matches("a.assets.example.org", 443) && p.matches("x.y.assets.example.org", 443));
        assert!(!p.matches("assets.example.org", 443));
        assert!(!p.matches("evilassets.example.org", 443));
    }

    #[test]
    fn ports_narrow_a_pattern() {
        let p = HostPattern::parse("127.0.0.1:8000").unwrap();
        assert!(p.matches("127.0.0.1", 8000) && !p.matches("127.0.0.1", 8001));
        let p = HostPattern::parse("[::1]:9000").unwrap();
        assert!(p.matches("::1", 9000) && !p.matches("::1", 80));
        assert!(HostPattern::parse("::1").unwrap().matches("::1", 80));
        assert!(HostPattern::parse("x.org:http").is_err());
        assert!(HostPattern::parse("*.").is_err() && HostPattern::parse("a.*.org").is_err());
    }

    #[test]
    fn headers_are_checked_without_echoing_values() {
        assert_eq!(Header::parse("X-Api-Key:  k1 ").unwrap(), Header { name: "X-Api-Key".into(), value: "k1".into() });
        let e = Header::parse("Bad Name: secret").unwrap_err();
        assert!(!e.contains("secret"), "{e}");
        let e = Header::parse("X: secret\r\nInjected: 1").unwrap_err();
        assert!(e.contains("line break") && !e.contains("secret"), "{e}");
        let e = Header::parse("secret-without-colon").unwrap_err();
        assert!(!e.contains("secret"), "{e}");
    }

    #[test]
    fn matching_rules_merge_with_the_most_specific_winning() {
        let mut rules = HeaderRules::default();
        rules.set("*.example.org", headers(&["Authorization: wild", "X-A: 1"])).unwrap();
        rules.set("cdn.example.org", headers(&["authorization: exact"])).unwrap();
        rules.set("*.cdn.example.org", headers(&["X-B: 2"])).unwrap();
        assert_eq!(lines(&rules.for_host("cdn.example.org", 443)), ["authorization: exact", "X-A: 1"]);
        assert_eq!(lines(&rules.for_host("a.cdn.example.org", 443)), ["Authorization: wild", "X-A: 1", "X-B: 2"]);
        assert!(rules.for_host("example.org", 443).is_empty());
        assert!(rules.for_host("other.net", 443).is_empty());
    }

    #[test]
    fn setting_a_pattern_replaces_or_removes_it() {
        let mut rules = HeaderRules::default();
        rules.set("h.org", headers(&["A: 1"])).unwrap();
        rules.set("H.org", headers(&["B: 2"])).unwrap();
        assert_eq!(lines(&rules.for_host("h.org", 443)), ["B: 2"]);
        rules.set("h.org", vec![]).unwrap();
        assert!(rules.for_host("h.org", 443).is_empty());
    }
}
