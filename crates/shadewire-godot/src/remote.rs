//! `load_material_async`: a worker thread per request fetches the document, its includes and its textures, and
//! compiles; the main thread turns the result into Godot resources. Workers talk to the main thread through a
//! channel of `Event`s, which the `Shadewire` object drains every frame.

use std::collections::{BTreeSet, HashMap};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, RecvTimeoutError, Sender};
use std::time::{Duration, Instant};

use godot::prelude::*;
use shadewire_gdshader::GdshaderOutput;
use shadewire_mtlx::{Resolved, Resolver};

use crate::cache::SharedCache;
use crate::compile::{compile, reference};
use crate::fetch::{CANCELLED, FetchOptions, HeadersCallback, Session, cache_policy};
use crate::headers::{Header, HeaderRules};
use crate::http::GodotHttp;
use crate::io::read_text;
use crate::location::is_url;
use crate::options::LoadOptions;

/// The options only `load_material_async` takes.
pub const NETWORK_OPTIONS: [&str; 10] = [
    "allow_http",
    "headers",
    "headers_all_hosts",
    "cache_policy",
    "cache_authenticated",
    "connect_timeout",
    "timeout",
    "max_redirects",
    "max_response_bytes",
    "max_material_bytes",
];

/// The network options out of `dictionary`, and what is left for `options_from`.
pub fn fetch_options_from(dictionary: &VarDictionary) -> Result<(VarDictionary, FetchOptions), String> {
    let mut rest = dictionary.duplicate_shallow();
    let mut options = FetchOptions::default();
    for key in NETWORK_OPTIONS {
        let Some(value) = rest.remove(key) else { continue };
        // Header values are secrets: the message shows the key only.
        let bad = |what: &str| format!("option {key}: expected {what}");
        let seconds = |value: &Variant| -> Result<Duration, String> {
            let s = value.try_to::<f64>().map_err(|_| bad("a number of seconds"))?;
            Duration::try_from_secs_f64(s).ok().filter(|d| !d.is_zero()).ok_or_else(|| bad("a positive number"))
        };
        let bytes = |value: &Variant| value.try_to::<i64>().ok().filter(|n| *n > 0).ok_or_else(|| bad("a byte count"));
        match key {
            "allow_http" => options.allow_http = value.try_to().map_err(|_| bad("a bool"))?,
            "headers_all_hosts" => options.headers_all_hosts = value.try_to().map_err(|_| bad("a bool"))?,
            "cache_authenticated" => options.cache_authenticated = value.try_to().map_err(|_| bad("a bool"))?,
            "headers" => {
                let lines = value.try_to::<PackedStringArray>().map_err(|_| bad("an array of \"Name: value\""))?;
                let lines: Vec<String> = lines.as_slice().iter().map(GString::to_string).collect();
                options.headers = Header::parse_all(lines.iter().map(String::as_str))?;
            }
            "cache_policy" => {
                let name = value.try_to::<GString>().map_err(|_| bad("a string"))?;
                options.cache_policy = cache_policy(&name.to_string())?;
            }
            "connect_timeout" => options.connect_timeout = seconds(&value)?,
            "timeout" => options.timeout = seconds(&value)?,
            "max_redirects" => {
                options.max_redirects = value.try_to::<u32>().map_err(|_| bad("a count"))?;
            }
            "max_response_bytes" => options.max_response_bytes = bytes(&value)? as u64,
            "max_material_bytes" => options.max_material_bytes = bytes(&value)? as u64,
            _ => unreachable!("listed in NETWORK_OPTIONS"),
        }
    }
    Ok((rest, options))
}

/// What a worker sends the main thread.
pub enum Event {
    Progress {
        id: i64,
        done: u64,
        total: u64,
    },
    /// The worker waits for the `http_headers_callback` result for `url`.
    Headers {
        url: String,
        reply: Sender<Vec<String>>,
    },
    Done {
        id: i64,
        result: Result<Box<Fetched>, String>,
    },
}

/// A compiled material and the bytes of its remote textures, ready for the main thread.
pub struct Fetched {
    pub out: GdshaderOutput,
    pub location: String,
    pub options: LoadOptions,
    /// By texture location: the bytes, or why they could not be fetched.
    pub textures: HashMap<String, Result<Vec<u8>, String>>,
}

pub struct Job {
    pub id: i64,
    pub source: String,
    pub options: LoadOptions,
    pub fetch: FetchOptions,
    pub rules: HeaderRules,
    pub has_callback: bool,
    pub cache: Option<SharedCache>,
    pub cancelled: Arc<AtomicBool>,
}

pub fn spawn(job: Job, events: Sender<Event>) {
    std::thread::Builder::new()
        .name(format!("shadewire-fetch-{}", job.id))
        .spawn(move || {
            let id = job.id;
            let cancelled = job.cancelled.clone();
            let result = run(job, &events);
            let result = if cancelled.load(Ordering::Relaxed) { Err(CANCELLED.to_string()) } else { result };
            let _ = events.send(Event::Done { id, result });
        })
        .expect("spawning a thread");
}

/// Asks the main thread for the callback's headers and waits for them, giving up on cancel or timeout.
fn callback(events: Sender<Event>, cancelled: Arc<AtomicBool>, timeout: Duration) -> HeadersCallback {
    Arc::new(move |url: &str| {
        let (reply, answer) = mpsc::channel();
        events.send(Event::Headers { url: url.to_string(), reply }).map_err(|_| CANCELLED.to_string())?;
        let deadline = Instant::now() + timeout;
        loop {
            match answer.recv_timeout(Duration::from_millis(10)) {
                Ok(lines) => return Header::parse_all(lines.iter().map(String::as_str)),
                Err(RecvTimeoutError::Disconnected) => return Err(CANCELLED.to_string()),
                Err(RecvTimeoutError::Timeout) if cancelled.load(Ordering::Relaxed) => return Err(CANCELLED.into()),
                Err(RecvTimeoutError::Timeout) if Instant::now() > deadline => {
                    return Err(format!("{url}: http_headers_callback did not answer"));
                }
                Err(RecvTimeoutError::Timeout) => {}
            }
        }
    })
}

fn run(job: Job, events: &Sender<Event>) -> Result<Box<Fetched>, String> {
    let Job { id, source, options, fetch, rules, has_callback, cache, cancelled } = job;
    let timeout = fetch.timeout;
    let mut session = Session::new(Arc::new(GodotHttp), rules, fetch);
    session.cache = cache;
    session.cancelled = cancelled.clone();
    let progress = events.clone();
    session.progress = Some(Arc::new(move |done, total| {
        let _ = progress.send(Event::Progress { id, done, total });
    }));
    if has_callback {
        session.callback = Some(callback(events.clone(), cancelled.clone(), timeout));
    }
    let session = Arc::new(session);

    let text = if is_url(&source) {
        session.set_document(&source);
        session.fetch_text(&source)?
    } else {
        read_text(&source)?
    };
    let includes = session.clone();
    let resolver = Resolver::new(move |base, href| {
        let location = reference(Some(base), href)?;
        let text = if is_url(&location) { includes.fetch_text(&location)? } else { read_text(&location)? };
        Ok(Resolved { location, text })
    });
    let out = compile(&text, Some(&source), &options, resolver)?;
    if cancelled.load(Ordering::Relaxed) {
        return Err(CANCELLED.into());
    }

    // Every remote texture at once; local ones load on the main thread.
    let urls: BTreeSet<String> = out
        .reflection
        .textures
        .iter()
        .filter_map(|t| reference(Some(&source), &t.file).ok())
        .filter(|location| is_url(location))
        .collect();
    let session = &session;
    let textures = std::thread::scope(|scope| {
        let fetches: Vec<_> = urls.into_iter().map(|url| scope.spawn(move || (session.fetch(&url), url))).collect();
        fetches.into_iter().map(|f| f.join().expect("a texture fetch panicked")).map(|(r, u)| (u, r)).collect()
    });
    Ok(Box::new(Fetched { out, location: source, options, textures }))
}
