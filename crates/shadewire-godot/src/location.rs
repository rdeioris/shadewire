//! Locations are strings: `res://…`, `user://…`, an absolute or relative OS path, or an `http(s)://` URL. Includes
//! and textures resolve against the location of the document that names them.

/// `http://` or `https://`.
pub fn is_url(location: &str) -> bool {
    let lower = location.get(..8).unwrap_or(location).to_ascii_lowercase();
    lower.starts_with("http://") || lower.starts_with("https://")
}

/// The `scheme://` prefix (`res://`, `https://host`), a Windows drive (`C:`) or a UNC host (`//server`), and the
/// rest, which starts at its first `/` (or is empty).
fn split_root(location: &str) -> (&str, &str) {
    if let Some(i) = location.find("://") {
        let after = i + 3;
        // A URL's authority belongs to the root; `res://` and `user://` have none.
        let authority =
            if is_url(location) { location[after..].find('/').map_or(location.len(), |j| after + j) } else { after };
        return (&location[..authority], &location[authority..]);
    }
    let bytes = location.as_bytes();
    if bytes.len() >= 2 && bytes[1] == b':' && bytes[0].is_ascii_alphabetic() {
        return (&location[..2], &location[2..]);
    }
    if let Some(rest) = location.strip_prefix("//") {
        let end = rest.find('/').map_or(location.len(), |j| 2 + j);
        return (&location[..end], &location[end..]);
    }
    ("", location)
}

/// Whether `href` names its own root: a scheme, a drive, a UNC host or a leading `/`.
pub fn is_absolute(href: &str) -> bool {
    let href = href.replace('\\', "/");
    let (root, path) = split_root(&href);
    !root.is_empty() || path.starts_with('/')
}

/// `.` and `..` segments removed (RFC 3986 section 5.2.4); `..` never climbs above the root.
fn remove_dot_segments(path: &str) -> String {
    let absolute = path.starts_with('/');
    let mut out: Vec<&str> = Vec::new();
    let segments: Vec<&str> = path.split('/').collect();
    for (i, segment) in segments.iter().enumerate() {
        let last = i + 1 == segments.len();
        match *segment {
            "." => {
                if last {
                    out.push("");
                }
            }
            ".." => {
                if out.last().is_some_and(|s| !s.is_empty() && *s != "..") {
                    out.pop();
                } else if !absolute {
                    out.push("..");
                }
                if last {
                    out.push("");
                }
            }
            "" if i > 0 && !last => {}
            s => out.push(s),
        }
    }
    let joined = out.join("/");
    if absolute && !joined.starts_with('/') { format!("/{joined}") } else { joined }
}

/// `href` resolved against the document at `base`: an absolute `href` stands alone, a relative one joins the
/// directory of `base` (everything up to its last `/`; a `base` ending in `/` is a directory). Backslashes become
/// `/`, and a URL's query and fragment do not count as directories.
pub fn join(base: &str, href: &str) -> String {
    let href = href.replace('\\', "/");
    if is_absolute(&href) {
        let (root, path) = split_root(&href);
        return format!("{root}{}", remove_dot_segments(path));
    }
    let base = base.replace('\\', "/");
    let base = if is_url(&base) { base.split(['?', '#']).next().unwrap_or("").to_string() } else { base };
    let (root, path) = split_root(&base);
    let dir = path.rfind('/').map_or("", |i| &path[..=i]);
    let dir = if dir.is_empty() && !root.is_empty() && !root.ends_with("//") { "/" } else { dir };
    format!("{root}{}", remove_dot_segments(&format!("{dir}{href}")))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn relative_references_join_the_documents_directory() {
        assert_eq!(join("res://materials/rust.mtlx", "tex/a.png"), "res://materials/tex/a.png");
        assert_eq!(join("res://rust.mtlx", "a.png"), "res://a.png");
        assert_eq!(join("user://mods/x/doc.mtlx", "../lib/inc.mtlx"), "user://mods/lib/inc.mtlx");
        assert_eq!(join("C:\\assets\\mats\\x.mtlx", "..\\tex\\a.png"), "C:/assets/tex/a.png");
        assert_eq!(join("/home/me/x.mtlx", "./a.png"), "/home/me/a.png");
        assert_eq!(join("x.mtlx", "a.png"), "a.png");
        assert_eq!(join("D:/mats/", "a.png"), "D:/mats/a.png");
    }

    #[test]
    fn urls_join_as_rfc_3986_says() {
        assert_eq!(join("https://cdn.example.com/m/doc.mtlx?v=2", "t/a.png"), "https://cdn.example.com/m/t/a.png");
        assert_eq!(join("https://cdn.example.com/m/doc.mtlx", "../../a.png"), "https://cdn.example.com/a.png");
        assert_eq!(join("https://cdn.example.com", "a.png"), "https://cdn.example.com/a.png");
        assert_eq!(join("https://cdn.example.com/materials/", "a.mtlx"), "https://cdn.example.com/materials/a.mtlx");
    }

    #[test]
    fn absolute_references_stand_alone() {
        assert_eq!(join("res://a/b.mtlx", "https://x.org/t.png"), "https://x.org/t.png");
        assert_eq!(join("https://x.org/a/b.mtlx", "user://t.png"), "user://t.png");
        assert_eq!(join("res://a/b.mtlx", "E:/t.png"), "E:/t.png");
        assert_eq!(join("res://a/b.mtlx", "/srv/t.png"), "/srv/t.png");
        assert!(is_url("HTTPS://x.org") && !is_url("res://x") && !is_url("C:/x"));
        assert!(is_absolute("res://x") && is_absolute("C:\\x") && is_absolute("//server/share") && !is_absolute("a/b"));
    }
}
