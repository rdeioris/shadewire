//! Fetching one material's documents and textures over http(s): the scheme policy, which headers go to which
//! host, redirects, size caps and the disk cache. The wire is a `Transport` (Godot's `HTTPClient` in the
//! extension, a fake in the tests). Errors name URLs, never header values.

use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use crate::cache::{Cached, SharedCache};
use crate::headers::{Header, HeaderRules, merge};
use crate::location::join;
use crate::options::Budget;
use crate::url::{Url, without_fragment};

/// One response, redirects not followed. Header names are lowercase.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Response {
    pub status: u16,
    pub headers: Vec<(String, String)>,
    pub body: Vec<u8>,
}

impl Response {
    pub fn header(&self, name: &str) -> Option<&str> {
        self.headers.iter().find(|(n, _)| n == name).map(|(_, v)| v.as_str())
    }
}

/// What a transport needs to honour for one request.
pub struct Wire<'a> {
    pub connect_timeout: Duration,
    pub deadline: Instant,
    pub max_bytes: u64,
    pub cancelled: &'a AtomicBool,
    /// `(bytes received, bytes announced)` since the last call: the length once when the headers arrive, then
    /// each chunk (a response of unknown length announces each chunk as it comes).
    pub progress: &'a (dyn Fn(u64, u64) + Sync),
}

impl Wire<'_> {
    /// Fails a request that ran out of time or was cancelled.
    pub fn check(&self, url: &str) -> Result<(), String> {
        if self.cancelled.load(Ordering::Relaxed) {
            Err(CANCELLED.into())
        } else if Instant::now() > self.deadline {
            Err(format!("{url}: timed out"))
        } else {
            Ok(())
        }
    }
}

pub const CANCELLED: &str = "cancelled";

pub trait Transport: Send + Sync {
    /// A GET of `url` with `headers`; the transport adds `Host` and nothing secret.
    fn get(&self, url: &Url, headers: &[Header], wire: &Wire) -> Result<Response, String>;
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CachePolicy {
    /// A cached copy is served without asking the server.
    Use,
    /// A cached copy is served after the server confirms it (`If-None-Match` / `If-Modified-Since`).
    Revalidate,
    /// The cache is not read; the fresh response replaces the cached one.
    Bypass,
    /// Only the cache: anything not cached fails.
    Offline,
}

pub fn cache_policy(name: &str) -> Result<CachePolicy, String> {
    match name {
        "use" => Ok(CachePolicy::Use),
        "revalidate" => Ok(CachePolicy::Revalidate),
        "bypass" => Ok(CachePolicy::Bypass),
        "offline" => Ok(CachePolicy::Offline),
        _ => Err(format!("cache_policy \"{name}\": expected use, revalidate, bypass or offline")),
    }
}

/// The network options of one `load_material_async`.
#[derive(Clone, Debug, PartialEq)]
pub struct FetchOptions {
    /// Plain `http://` is refused unless this is set.
    pub allow_http: bool,
    /// Sent to the document's own host, or to every host with `headers_all_hosts`.
    pub headers: Vec<Header>,
    pub headers_all_hosts: bool,
    pub cache_policy: CachePolicy,
    /// Responses fetched with headers are cached too, unless this is false.
    pub cache_authenticated: bool,
    pub connect_timeout: Duration,
    /// Per request, redirects included.
    pub timeout: Duration,
    pub max_redirects: u32,
    pub max_response_bytes: u64,
}

impl Default for FetchOptions {
    fn default() -> FetchOptions {
        FetchOptions {
            allow_http: false,
            headers: Vec::new(),
            headers_all_hosts: false,
            cache_policy: CachePolicy::Revalidate,
            cache_authenticated: true,
            connect_timeout: Duration::from_secs(10),
            timeout: Duration::from_secs(60),
            max_redirects: 5,
            max_response_bytes: 64 * 1024 * 1024,
        }
    }
}

/// Extra headers for a URL, asked per request (`http_headers_callback`).
pub type HeadersCallback = Arc<dyn Fn(&str) -> Result<Vec<Header>, String> + Send + Sync>;

/// Progress of a whole material: bytes received and bytes known to come.
pub type ProgressFn = Arc<dyn Fn(u64, u64) + Send + Sync>;

/// The fetches of one material. Shared by the threads that fetch its includes and textures.
pub struct Session {
    pub transport: Arc<dyn Transport>,
    pub rules: HeaderRules,
    pub callback: Option<HeadersCallback>,
    pub options: FetchOptions,
    pub cache: Option<SharedCache>,
    pub cancelled: Arc<AtomicBool>,
    pub progress: Option<ProgressFn>,
    /// The material's bytes, local sources included (`max_material_bytes`); unlimited unless set.
    pub budget: Arc<Budget>,
    /// The host and port of the document: where `options.headers` go.
    document: Mutex<Option<(String, u16)>>,
    done: AtomicU64,
    total: AtomicU64,
    pinned: Mutex<Vec<String>>,
}

const REDIRECTS: [u16; 5] = [301, 302, 303, 307, 308];

impl Session {
    pub fn new(transport: Arc<dyn Transport>, rules: HeaderRules, options: FetchOptions) -> Session {
        Session {
            transport,
            rules,
            callback: None,
            options,
            cache: None,
            cancelled: Arc::default(),
            progress: None,
            budget: Arc::default(),
            document: Mutex::new(None),
            done: AtomicU64::new(0),
            total: AtomicU64::new(0),
            pinned: Mutex::new(Vec::new()),
        }
    }

    /// Names the document's location; per-request headers go to its host.
    pub fn set_document(&self, location: &str) {
        if let Ok(url) = Url::parse(location) {
            *self.document.lock().unwrap() = Some((url.host, url.port));
        }
    }

    fn cache<T>(&self, f: impl FnOnce(&mut crate::cache::DiskCache) -> T) -> Option<T> {
        self.cache.as_ref().map(|cache| f(&mut cache.lock().unwrap_or_else(|e| e.into_inner())))
    }

    fn report(&self, done: u64, total: u64) {
        let done = self.done.fetch_add(done, Ordering::Relaxed) + done;
        let total = self.total.fetch_add(total, Ordering::Relaxed) + total;
        if let Some(progress) = &self.progress {
            progress(done, total);
        }
    }

    /// The headers for one hop. `origin` is the host the request was first made to: per-request headers follow a
    /// redirect only on that host; host rules and the callback are matched against each hop's own URL.
    fn headers(&self, hop: &Url, hop_url: &str, origin: (&str, u16)) -> Result<Vec<Header>, String> {
        let mut layers = vec![self.rules.for_host(&hop.host, hop.port)];
        let document = self.document.lock().unwrap().clone();
        let to_document = document.as_ref().is_some_and(|(h, p)| hop.origin() == (h.as_str(), *p));
        if hop.origin() == origin && (self.options.headers_all_hosts || to_document) {
            layers.push(self.options.headers.clone());
        }
        if let Some(callback) = &self.callback {
            layers.push(callback(hop_url)?);
        }
        Ok(merge(layers))
    }

    fn serve(&self, key: &str, cached: Cached) -> Result<Vec<u8>, String> {
        self.budget.charge(key, cached.body.len() as u64)?;
        let size = cached.body.len() as u64;
        self.report(size, size);
        Ok(cached.body)
    }

    /// The body at `url` (cached, revalidated or fetched).
    pub fn fetch(&self, url: &str) -> Result<Vec<u8>, String> {
        let key = without_fragment(url).to_string();
        let first = Url::parse(&key)?;
        self.check_scheme(&key, &first, None)?;
        let policy = self.options.cache_policy;
        let cached = if policy == CachePolicy::Bypass { None } else { self.cache(|c| c.get(&key)).flatten() };
        if self.cache.is_some() {
            self.cache(|c| c.pin(&key));
            self.pinned.lock().unwrap().push(key.clone());
        }
        match (policy, cached) {
            (CachePolicy::Use | CachePolicy::Offline, Some(cached)) => self.serve(&key, cached),
            (CachePolicy::Offline, None) => Err(format!("{key}: not cached (cache_policy offline)")),
            (_, cached) => self.download(&key, first, cached),
        }
    }

    fn check_scheme(&self, key: &str, url: &Url, from: Option<&Url>) -> Result<(), String> {
        if url.https || self.options.allow_http {
            return Ok(());
        }
        if from.is_some_and(|f| f.https) {
            return Err(format!("{key}: redirected from https to plain http"));
        }
        Err(format!("{key}: plain http is refused (set the allow_http option)"))
    }

    fn download(&self, key: &str, first: Url, cached: Option<Cached>) -> Result<Vec<u8>, String> {
        let wire_progress = |done: u64, total: u64| self.report(done, total);
        let wire = Wire {
            connect_timeout: self.options.connect_timeout,
            deadline: Instant::now() + self.options.timeout,
            max_bytes: self.options.max_response_bytes,
            cancelled: &self.cancelled,
            progress: &wire_progress,
        };
        let origin = first.clone();
        let (mut hop, mut hop_url) = (first, key.to_string());
        let mut authenticated = false;
        for _ in 0..=self.options.max_redirects {
            wire.check(key)?;
            let mut headers = self.headers(&hop, &hop_url, origin.origin())?;
            authenticated |= !headers.is_empty();
            if let Some(cached) = &cached {
                let validators = [("If-None-Match", &cached.etag), ("If-Modified-Since", &cached.last_modified)];
                for (name, value) in validators {
                    if let Some(value) = value {
                        headers.push(Header { name: name.into(), value: value.clone() });
                    }
                }
            }
            let response = self.transport.get(&hop, &headers, &wire)?;
            match response.status {
                status if REDIRECTS.contains(&status) => {
                    let location = response
                        .header("location")
                        .ok_or_else(|| format!("{hop_url}: redirect {status} without a Location"))?;
                    let next_url = without_fragment(&join(&hop_url, location)).to_string();
                    let next = Url::parse(&next_url)?;
                    self.check_scheme(&next_url, &next, Some(&hop))?;
                    (hop, hop_url) = (next, next_url);
                }
                304 => {
                    let cached = cached.ok_or_else(|| format!("{key}: 304 Not Modified with nothing cached"))?;
                    return self.serve(key, cached);
                }
                200 => {
                    self.budget.charge(key, response.body.len() as u64)?;
                    let no_store = response
                        .header("cache-control")
                        .is_some_and(|c| c.split(',').any(|d| d.trim().eq_ignore_ascii_case("no-store")));
                    if !no_store && (self.options.cache_authenticated || !authenticated) {
                        let etag = response.header("etag").map(str::to_string);
                        let last_modified = response.header("last-modified").map(str::to_string);
                        self.cache(|c| c.put(key, &response.body, etag, last_modified));
                    }
                    return Ok(response.body);
                }
                status => return Err(format!("{hop_url}: HTTP {status}")),
            }
        }
        Err(format!("{key}: more than {} redirects", self.options.max_redirects))
    }

    pub fn fetch_text(&self, url: &str) -> Result<String, String> {
        String::from_utf8(self.fetch(url)?).map_err(|_| format!("{} is not UTF-8", without_fragment(url)))
    }
}

impl Drop for Session {
    fn drop(&mut self) {
        let pinned = std::mem::take(&mut *self.pinned.lock().unwrap());
        self.cache(|cache| pinned.iter().for_each(|key| cache.unpin(key)));
    }
}

#[cfg(test)]
pub mod tests {
    use std::collections::HashMap;

    use super::*;
    use crate::cache::DiskCache;

    type Handler = Box<dyn Fn(&[Header]) -> Response + Send + Sync>;

    /// Canned responses by URL; every request is recorded with its header lines.
    #[derive(Default)]
    pub struct Fake {
        routes: HashMap<String, Handler>,
        pub log: Mutex<Vec<(String, Vec<String>)>>,
    }

    impl Fake {
        fn route(mut self, url: &str, f: impl Fn(&[Header]) -> Response + Send + Sync + 'static) -> Fake {
            self.routes.insert(url.into(), Box::new(f));
            self
        }

        fn ok(self, url: &str, body: &'static str) -> Fake {
            self.route(url, move |_| Response { status: 200, headers: vec![], body: body.into() })
        }

        fn redirect(self, url: &str, to: &'static str) -> Fake {
            self.route(url, move |_| Response {
                status: 302,
                headers: vec![("location".into(), to.into())],
                body: vec![],
            })
        }

        fn requests(&self) -> Vec<(String, Vec<String>)> {
            self.log.lock().unwrap().clone()
        }
    }

    impl Transport for Fake {
        fn get(&self, url: &Url, headers: &[Header], wire: &Wire) -> Result<Response, String> {
            let full = url.to_string();
            self.log.lock().unwrap().push((full.clone(), headers.iter().map(Header::line).collect()));
            let response =
                self.routes.get(&full).map_or(Response { status: 404, ..Default::default() }, |f| f(headers));
            (wire.progress)(0, response.body.len() as u64);
            (wire.progress)(response.body.len() as u64, 0);
            Ok(response)
        }
    }

    fn session(fake: Fake, rules: HeaderRules, options: FetchOptions) -> (Session, Arc<Fake>) {
        let fake = Arc::new(fake);
        (Session::new(fake.clone(), rules, options), fake)
    }

    fn rules(pattern: &str, lines: &[&str]) -> HeaderRules {
        let mut rules = HeaderRules::default();
        rules.set(pattern, Header::parse_all(lines.iter().copied()).unwrap()).unwrap();
        rules
    }

    #[test]
    fn plain_http_needs_the_option() {
        let (s, _) = session(Fake::default().ok("http://h/a", "x"), HeaderRules::default(), FetchOptions::default());
        assert!(s.fetch("http://h/a").unwrap_err().contains("plain http is refused"));
        let options = FetchOptions { allow_http: true, ..FetchOptions::default() };
        let (s, _) = session(Fake::default().ok("http://h/a", "x"), HeaderRules::default(), options);
        assert_eq!(s.fetch("http://h/a#frag").unwrap(), b"x");
    }

    #[test]
    fn scoped_headers_reach_their_host_only() {
        let fake = Fake::default().ok("https://cdn.x.org/doc.mtlx", "d").ok("https://other.net/t.png", "t");
        let (s, fake) = session(fake, rules("cdn.x.org", &["Authorization: Bearer s3cret"]), FetchOptions::default());
        s.fetch("https://cdn.x.org/doc.mtlx").unwrap();
        s.fetch("https://other.net/t.png").unwrap();
        let log = fake.requests();
        assert_eq!(log[0].1, ["Authorization: Bearer s3cret"]);
        assert!(log[1].1.is_empty());
    }

    #[test]
    fn cross_host_redirects_drop_scoped_and_per_request_headers() {
        let fake = Fake::default()
            .redirect("https://cdn.x.org/a.png", "https://mirror.net/a.png")
            .ok("https://mirror.net/a.png", "png")
            .redirect("https://cdn.x.org/b.png", "/real/b.png")
            .ok("https://cdn.x.org/real/b.png", "png");
        let options = FetchOptions { headers: Header::parse_all(["X-Req: 1"]).unwrap(), ..FetchOptions::default() };
        let (s, fake) = session(fake, rules("cdn.x.org", &["Authorization: t"]), options);
        s.set_document("https://cdn.x.org/doc.mtlx");
        assert_eq!(s.fetch("https://cdn.x.org/a.png").unwrap(), b"png");
        assert_eq!(s.fetch("https://cdn.x.org/b.png").unwrap(), b"png");
        let log = fake.requests();
        assert_eq!(log[0].1, ["Authorization: t", "X-Req: 1"]);
        assert_eq!(log[1], ("https://mirror.net/a.png".to_string(), vec![]));
        // A redirect on the same host keeps them.
        assert_eq!(log[3], ("https://cdn.x.org/real/b.png".to_string(), log[0].1.clone()));
    }

    #[test]
    fn per_request_headers_stay_on_the_documents_host_unless_all_hosts() {
        let fake = || Fake::default().ok("https://d.org/doc.mtlx", "d").ok("https://t.org/t.png", "t");
        let headers = Header::parse_all(["X-Req: 1"]).unwrap();
        let options = FetchOptions { headers: headers.clone(), ..FetchOptions::default() };
        let (s, f) = session(fake(), HeaderRules::default(), options.clone());
        s.set_document("https://d.org/doc.mtlx");
        s.fetch("https://d.org/doc.mtlx").unwrap();
        s.fetch("https://t.org/t.png").unwrap();
        assert_eq!(f.requests().iter().map(|r| r.1.len()).collect::<Vec<_>>(), [1, 0]);
        let (s, f) = session(fake(), HeaderRules::default(), FetchOptions { headers_all_hosts: true, ..options });
        s.set_document("https://d.org/doc.mtlx");
        s.fetch("https://t.org/t.png").unwrap();
        assert_eq!(f.requests()[0].1, ["X-Req: 1"]);
    }

    #[test]
    fn the_callback_adds_headers_per_url() {
        let fake = Fake::default().ok("https://a.org/x", "x");
        let (mut s, f) = session(fake, rules("a.org", &["A: static", "B: static"]), FetchOptions::default());
        s.callback = Some(Arc::new(|url: &str| Header::parse_all([format!("B: signed {url}").as_str()])));
        s.fetch("https://a.org/x").unwrap();
        assert_eq!(f.requests()[0].1, ["A: static", "B: signed https://a.org/x"]);
    }

    #[test]
    fn redirects_are_limited_and_never_downgrade() {
        let fake = Fake::default().redirect("https://a.org/1", "/2").redirect("https://a.org/2", "/1");
        let options = FetchOptions { max_redirects: 3, ..FetchOptions::default() };
        let (s, f) = session(fake, HeaderRules::default(), options);
        let e = s.fetch("https://a.org/1").unwrap_err();
        assert!(e.contains("more than 3 redirects"), "{e}");
        assert_eq!(f.requests().len(), 4);
        let fake = Fake::default().redirect("https://a.org/x", "http://a.org/x");
        let (s, _) =
            session(fake, HeaderRules::default(), FetchOptions { allow_http: false, ..FetchOptions::default() });
        assert!(s.fetch("https://a.org/x").unwrap_err().contains("from https to plain http"));
    }

    #[test]
    fn errors_never_carry_header_values() {
        let fake = Fake::default().route("https://a.org/x", |_| Response { status: 403, ..Default::default() });
        let (s, _) = session(fake, rules("a.org", &["Authorization: s3cret"]), FetchOptions::default());
        let e = s.fetch("https://a.org/x").unwrap_err();
        assert!(e.contains("HTTP 403") && !e.contains("s3cret"), "{e}");
    }

    #[test]
    fn the_material_size_cap_spans_every_fetch() {
        let fake = Fake::default().ok("https://a.org/1", "12345").ok("https://a.org/2", "12345");
        let (mut s, _) = session(fake, HeaderRules::default(), FetchOptions::default());
        s.budget = Budget::new(Some(8));
        s.fetch("https://a.org/1").unwrap();
        assert!(s.fetch("https://a.org/2").unwrap_err().contains("max_material_bytes"));
    }

    fn cached_session(fake: Fake, dir: &std::path::Path, options: FetchOptions) -> (Session, Arc<Fake>) {
        let (mut s, f) = session(fake, rules("a.org", &["Authorization: t"]), options);
        s.cache = Some(Arc::new(Mutex::new(DiskCache::open(dir, 1 << 20))));
        (s, f)
    }

    #[test]
    fn cached_entries_revalidate_with_their_validators() {
        let dir = std::env::temp_dir().join(format!("shadewire-fetch-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let fake = || {
            Fake::default().route("https://a.org/x", |headers| {
                if headers.iter().any(|h| h.name == "If-None-Match" && h.value == "\"v1\"") {
                    Response { status: 304, ..Default::default() }
                } else {
                    Response { status: 200, headers: vec![("etag".into(), "\"v1\"".into())], body: b"body".to_vec() }
                }
            })
        };
        let (s, f) = cached_session(fake(), &dir, FetchOptions::default());
        assert_eq!(s.fetch("https://a.org/x").unwrap(), b"body");
        drop(s);
        // A second session on the same directory: the entry is on disk, the server answers 304.
        let (s, f2) = cached_session(fake(), &dir, FetchOptions::default());
        assert_eq!(s.fetch("https://a.org/x").unwrap(), b"body");
        assert_eq!(f2.requests()[0].1, ["Authorization: t", "If-None-Match: \"v1\""]);
        assert_eq!(f.requests().len(), 1);
        // use: no request at all; offline: the same; bypass: a plain GET.
        let (s, f3) =
            cached_session(fake(), &dir, FetchOptions { cache_policy: CachePolicy::Use, ..Default::default() });
        assert_eq!(s.fetch("https://a.org/x").unwrap(), b"body");
        assert!(f3.requests().is_empty());
        let (s, f4) =
            cached_session(fake(), &dir, FetchOptions { cache_policy: CachePolicy::Bypass, ..Default::default() });
        assert_eq!(s.fetch("https://a.org/x").unwrap(), b"body");
        assert_eq!(f4.requests()[0].1, ["Authorization: t"]);
        let (s, _) = cached_session(
            Fake::default(),
            &dir,
            FetchOptions { cache_policy: CachePolicy::Offline, ..Default::default() },
        );
        assert!(s.fetch("https://a.org/y").unwrap_err().contains("not cached"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn no_store_and_unauthenticated_only_responses_skip_the_cache() {
        let dir = std::env::temp_dir().join(format!("shadewire-fetch-nostore-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let fake = Fake::default()
            .route("https://a.org/n", |_| Response {
                status: 200,
                headers: vec![("cache-control".into(), "private, no-store".into())],
                body: b"n".to_vec(),
            })
            .ok("https://a.org/auth", "a")
            .ok("https://b.org/open", "o");
        let options = FetchOptions { cache_authenticated: false, ..FetchOptions::default() };
        let (s, _) = cached_session(fake, &dir, options);
        for url in ["https://a.org/n", "https://a.org/auth", "https://b.org/open"] {
            s.fetch(url).unwrap();
        }
        let cache = s.cache.clone().unwrap();
        assert_eq!(cache.lock().unwrap().len(), 1);
        assert!(cache.lock().unwrap().get("https://b.org/open").is_some());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn progress_counts_the_whole_material() {
        let fake = Fake::default().ok("https://a.org/1", "123").ok("https://a.org/2", "4567");
        let (mut s, _) = session(fake, HeaderRules::default(), FetchOptions::default());
        let seen = Arc::new(Mutex::new(Vec::new()));
        let sink = seen.clone();
        s.progress = Some(Arc::new(move |done, total| sink.lock().unwrap().push((done, total))));
        s.fetch("https://a.org/1").unwrap();
        s.fetch("https://a.org/2").unwrap();
        assert_eq!(seen.lock().unwrap().last(), Some(&(7, 7)));
    }
}
