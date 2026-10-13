//! `http(s)://` URLs split into what a request needs: the scheme, the host and port to connect to, and the request
//! target. The fragment never leaves the client.

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Url {
    pub https: bool,
    /// Lowercase, without the brackets of an IPv6 literal and without a trailing dot.
    pub host: String,
    pub port: u16,
    /// The path and query, starting with `/`.
    pub target: String,
}

impl Url {
    pub fn parse(url: &str) -> Result<Url, String> {
        let bad = |why: &str| format!("{}: {why}", without_fragment(url));
        let (https, rest) = match url.find("://").map(|i| (url[..i].to_ascii_lowercase(), &url[i + 3..])) {
            Some((scheme, rest)) if scheme == "https" => (true, rest),
            Some((scheme, rest)) if scheme == "http" => (false, rest),
            _ => return Err(bad("not an http(s) URL")),
        };
        let rest = without_fragment(rest);
        let end = rest.find(['/', '?']).unwrap_or(rest.len());
        let (authority, target) = rest.split_at(end);
        if authority.contains('@') {
            // Not echoed: the URL holds a secret.
            return Err("a URL with credentials is not supported (use set_http_headers)".into());
        }
        let (host, port) = if let Some(literal) = authority.strip_prefix('[') {
            let close = literal.find(']').ok_or_else(|| bad("unterminated IPv6 address"))?;
            (&literal[..close], literal[close + 1..].strip_prefix(':'))
        } else {
            match authority.rsplit_once(':') {
                Some((host, port)) => (host, Some(port)),
                None => (authority, None),
            }
        };
        let host = host.trim_end_matches('.').to_ascii_lowercase();
        if host.is_empty() {
            return Err(bad("no host"));
        }
        let port = match port {
            Some(port) => port.parse::<u16>().map_err(|_| bad("bad port"))?,
            None if https => 443,
            None => 80,
        };
        let target = if target.starts_with('/') { target.to_string() } else { format!("/{target}") };
        Ok(Url { https, host, port, target })
    }

    /// The port is the scheme's default.
    pub fn default_port(&self) -> bool {
        self.port == if self.https { 443 } else { 80 }
    }

    /// Host and port, as the cache and the redirect rules compare them.
    pub fn origin(&self) -> (&str, u16) {
        (&self.host, self.port)
    }
}

impl std::fmt::Display for Url {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        let scheme = if self.https { "https" } else { "http" };
        let host = if self.host.contains(':') { format!("[{}]", self.host) } else { self.host.clone() };
        let port = if self.default_port() { String::new() } else { format!(":{}", self.port) };
        write!(f, "{scheme}://{host}{port}{}", self.target)
    }
}

/// `url` without its `#fragment`.
pub fn without_fragment(url: &str) -> &str {
    url.split('#').next().unwrap_or(url)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn urls_split_into_host_port_and_target() {
        let url = Url::parse("HTTPS://CDN.Example.com./m/a.mtlx?v=2#frag").unwrap();
        assert_eq!(url, Url { https: true, host: "cdn.example.com".into(), port: 443, target: "/m/a.mtlx?v=2".into() });
        assert!(url.default_port());
        let url = Url::parse("http://127.0.0.1:8080").unwrap();
        assert_eq!((url.host.as_str(), url.port, url.target.as_str()), ("127.0.0.1", 8080, "/"));
        let url = Url::parse("http://[::1]:9000/x?q").unwrap();
        assert_eq!((url.host.as_str(), url.port, url.target.as_str()), ("::1", 9000, "/x?q"));
        assert_eq!(Url::parse("https://h?q=1").unwrap().target, "/?q=1");
        assert_eq!(Url::parse("HTTP://[::1]:80/a#f").unwrap().to_string(), "http://[::1]/a");
        assert_eq!(Url::parse("https://h:8443/a?b").unwrap().to_string(), "https://h:8443/a?b");
    }

    #[test]
    fn bad_urls_are_refused() {
        assert!(Url::parse("ftp://x.org/a").unwrap_err().contains("not an http(s) URL"));
        assert!(Url::parse("https://user:pw@x.org/a").unwrap_err().contains("credentials"));
        assert!(Url::parse("https://x.org:99999/").unwrap_err().contains("bad port"));
        assert!(Url::parse("https:///a").unwrap_err().contains("no host"));
    }
}
