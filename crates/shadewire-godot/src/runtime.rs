//! The runtime API: the `Shadewire` class, and the load both it and the resource loader run.

use std::collections::HashMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, Sender};

use godot::classes::{DirAccess, Engine, IRefCounted, ProjectSettings, RefCounted, SceneTree, ShaderMaterial};
use godot::prelude::*;

use crate::cache::{self, DEFAULT_MAX_BYTES, SharedCache};
use crate::compile::compile;
use crate::fetch::CANCELLED;
use crate::headers::{Header, HeaderRules};
use crate::io::{load_texture, read_text, resolver, texture_from_bytes};
use crate::material::{Built, build};
use crate::options::{LoadOptions, Models, limit, models, transparency};
use crate::remote::{Event, Fetched, Job, NETWORK_OPTIONS, fetch_options_from, spawn};
use crate::url::without_fragment;

/// Options from a GDScript dictionary: `material`, `models` (`"mapped"`, `"lobes"` or an array of model names),
/// `bake_constants`, `optimize`, `transparency`, `emission_scale`, `strict_textures`, and the limits `max_nodes`,
/// `max_textures`, `max_loop_iterations` (0 turns one off). Unknown keys are errors.
pub fn options_from(dictionary: &VarDictionary) -> Result<LoadOptions, String> {
    let mut options = LoadOptions::default();
    for (key, value) in dictionary.iter_shared() {
        let key = key.to_string();
        let bad = |what: &str| format!("option {key}: expected {what}, got {value}");
        match key.as_str() {
            "material" => {
                let name = value.try_to::<GString>().map_err(|_| bad("a string"))?.to_string();
                options.material = (!name.is_empty()).then_some(name);
            }
            "models" => {
                options.models = match value.try_to::<GString>() {
                    Ok(name) => models(&name.to_string())?,
                    Err(_) => Models::List(
                        value
                            .try_to::<PackedStringArray>()
                            .map_err(|_| bad("a string or an array of strings"))?
                            .as_slice()
                            .iter()
                            .map(GString::to_string)
                            .collect(),
                    ),
                }
            }
            "bake_constants" => options.bake_constants = value.try_to().map_err(|_| bad("a bool"))?,
            "optimize" => options.optimize = value.try_to().map_err(|_| bad("a bool"))?,
            "strict_textures" => options.strict_textures = value.try_to().map_err(|_| bad("a bool"))?,
            "transparency" => {
                let name = value.try_to::<GString>().map_err(|_| bad("a string"))?;
                options.transparency = transparency(&name.to_string())?;
            }
            "emission_scale" => {
                options.emission_scale = value.try_to::<f64>().map_err(|_| bad("a number"))? as f32;
            }
            "max_nodes" | "max_textures" | "max_loop_iterations" => {
                let n = value.try_to::<i64>().map_err(|_| bad("a count, 0 for none"))?;
                let parsed = match key.as_str() {
                    "max_nodes" => limit(n).map(|l| options.limits.max_nodes = l),
                    "max_textures" => limit(n).map(|l| options.limits.max_textures = l),
                    _ => limit(n).map(|l| options.limits.max_loop_iterations = l),
                };
                parsed.ok_or_else(|| bad("a count, 0 for none"))?;
            }
            _ if NETWORK_OPTIONS.contains(&key.as_str()) => {
                return Err(format!("option {key} applies to load_material_async only"));
            }
            _ => return Err(format!("unknown option {key}")),
        }
    }
    Ok(options)
}

/// Compile `text`, named by `location`, into a material. Texture failures that are not fatal are pushed as
/// warnings.
pub fn load_text(text: &str, location: Option<&str>, options: &LoadOptions) -> Result<Gd<ShaderMaterial>, String> {
    let out = compile(text, location, options, resolver())?;
    let built = build(&out, location, options.strict_textures, true, load_texture)?;
    Ok(warn(built, location.unwrap_or("<string>")))
}

fn warn(built: Built, name: &str) -> Gd<ShaderMaterial> {
    for warning in built.warnings {
        godot_warn!("shadewire: {name}: {warning}");
    }
    built.material
}

pub fn load_path(path: &str, options: &LoadOptions) -> Result<Gd<ShaderMaterial>, String> {
    let text = read_text(path)?;
    load_text(&text, Some(path), options)
}

/// The main thread's half of `load_material_async`: the resources, from the fetched texture bytes.
fn finish_fetched(fetched: Box<Fetched>) -> Result<Gd<ShaderMaterial>, String> {
    let Fetched { out, location, options, textures } = *fetched;
    let texture = |path: &str| match textures.get(path) {
        Some(Ok(bytes)) => texture_from_bytes(path, &PackedByteArray::from(bytes.as_slice())),
        Some(Err(e)) => Err(e.clone()),
        None => load_texture(path),
    };
    let built = build(&out, Some(&location), options.strict_textures, true, texture)?;
    Ok(warn(built, &location))
}

/// The options argument: a dictionary that can be a `#[opt]` default, which gdext allows only for types that
/// callers cannot mutate (the default is made read-only).
#[derive(Clone, Debug, Default)]
pub struct Options(VarDictionary);

impl GodotConvert for Options {
    type Via = VarDictionary;

    fn godot_shape() -> godot::meta::shape::GodotShape {
        VarDictionary::godot_shape()
    }
}

impl ToGodot for Options {
    type Pass = godot::meta::conv::ByValue;

    fn to_godot(&self) -> VarDictionary {
        self.0.clone()
    }
}

impl FromGodot for Options {
    fn try_from_godot(via: VarDictionary) -> Result<Options, godot::meta::error::ConvertError> {
        Ok(Options(via))
    }
}

// SAFETY: the default is a fresh, read-only dictionary per call: no caller can mutate or share it.
unsafe impl godot::meta::GodotImmutable for Options {
    fn into_runtime_immutable(self) -> Options {
        Options(self.0.into_read_only())
    }
}

const POLL: &str = "_shadewire_poll";

/// Loads MaterialX documents as `ShaderMaterial`s at runtime: from `res://`, `user://` or any OS path, and with
/// `load_material_async` from `http(s)://` URLs too.
///
/// ```gdscript
/// var sw := Shadewire.new()
/// var mat := sw.load_material("D:/assets/rusty_metal.mtlx", {"material": "Rust"})
/// if mat == null:
///     push_error(sw.get_last_error())
/// ```
#[derive(GodotClass)]
#[class(base=RefCounted)]
pub struct Shadewire {
    base: Base<RefCounted>,
    last_error: GString,
    /// `func(url: String) -> PackedStringArray`: extra `Name: value` headers for each request (signed URLs,
    /// short-lived tokens), added after the `set_http_headers` ones. Called on the main thread.
    #[var]
    http_headers_callback: Callable,
    /// Where the disk cache lives (default `user://shadewire_cache`). Objects sharing a directory share its cache.
    #[var]
    cache_dir: GString,
    /// The disk cache's cap in bytes (default 256 MiB); lowering it evicts least recently used entries, 0 turns the
    /// cache off. It belongs to the directory: every object using it sees the same cap.
    #[var(get = get_cache_max_bytes, set = set_cache_max_bytes)]
    cache_max_bytes: PhantomVar<i64>,
    rules: HeaderRules,
    requests: HashMap<i64, Arc<AtomicBool>>,
    next_id: i64,
    sender: Sender<Event>,
    receiver: Receiver<Event>,
}

#[godot_api]
impl IRefCounted for Shadewire {
    fn init(base: Base<RefCounted>) -> Shadewire {
        let (sender, receiver) = mpsc::channel();
        Shadewire {
            base,
            last_error: GString::new(),
            http_headers_callback: Callable::invalid(),
            cache_dir: "user://shadewire_cache".into(),
            cache_max_bytes: PhantomVar::default(),
            rules: HeaderRules::default(),
            requests: HashMap::new(),
            next_id: 1,
            sender,
            receiver,
        }
    }
}

impl Drop for Shadewire {
    fn drop(&mut self) {
        for cancelled in self.requests.values() {
            cancelled.store(true, Ordering::Relaxed);
        }
    }
}

impl Shadewire {
    fn finish<T>(&mut self, result: Result<T, String>) -> Option<T> {
        match result {
            Ok(value) => {
                self.last_error = GString::new();
                Some(value)
            }
            Err(e) => {
                self.last_error = e.as_str().into();
                None
            }
        }
    }

    fn cache(&self) -> SharedCache {
        let dir = ProjectSettings::singleton().globalize_path(&self.cache_dir).to_string();
        cache::shared(std::path::Path::new(&dir), DEFAULT_MAX_BYTES)
    }

    fn with_cache<T>(&self, f: impl FnOnce(&mut cache::DiskCache) -> T) -> T {
        f(&mut self.cache().lock().unwrap_or_else(|e| e.into_inner()))
    }

    /// Events are drained once per frame, from the scene tree's `process_frame`, while requests are pending.
    fn poll_while_pending(&mut self) -> Result<(), String> {
        let tree = Engine::singleton().get_main_loop().and_then(|l| l.try_cast::<SceneTree>().ok());
        let mut tree = tree.ok_or("load_material_async needs a SceneTree main loop")?;
        let callable = Callable::from_object_method(&self.to_gd(), POLL);
        if self.requests.is_empty() {
            if tree.is_connected("process_frame", &callable) {
                tree.disconnect("process_frame", &callable);
            }
        } else if !tree.is_connected("process_frame", &callable) {
            tree.connect("process_frame", &callable);
        }
        Ok(())
    }
}

#[godot_api]
impl Shadewire {
    /// `load_material_async` finished: the material, its textures bound.
    #[signal]
    fn material_ready(id: i64, material: Gd<ShaderMaterial>);

    /// `load_material_async` failed, or was cancelled (the message is then "cancelled").
    #[signal]
    fn material_failed(id: i64, message: GString);

    /// Bytes received so far for the request, and bytes known to come (it grows as includes and textures appear).
    #[signal]
    fn material_progress(id: i64, done: i64, total: i64);

    /// The document at `path` as a material, or null (see `get_last_error`). Includes and textures resolve
    /// relative to `path`. Local sources only: URLs load with `load_material_async`.
    #[func]
    fn load_material(
        &mut self,
        path: GString,
        #[opt(default = Options::default())] options: Options,
    ) -> Option<Gd<ShaderMaterial>> {
        let result = options_from(&options.0).and_then(|options| load_path(&path.to_string(), &options));
        self.finish(result)
    }

    /// Loads `source` (a URL or a local path) off the main thread and returns the request id; the result arrives
    /// as `material_ready` or `material_failed`, with `material_progress` meanwhile. Keep this object alive until
    /// then. Options: the `load_material` ones plus the network ones (`allow_http`, `headers`,
    /// `headers_all_hosts`, `cache_policy`, `cache_authenticated`, `connect_timeout`, `timeout`, `max_redirects`,
    /// `max_response_bytes`, `max_material_bytes`).
    #[func]
    fn load_material_async(&mut self, source: GString, #[opt(default = Options::default())] options: Options) -> i64 {
        let id = self.next_id;
        self.next_id += 1;
        let cancelled = Arc::new(AtomicBool::new(false));
        self.requests.insert(id, cancelled.clone());
        let parsed = fetch_options_from(&options.0).and_then(|(rest, fetch)| Ok((options_from(&rest)?, fetch)));
        match self.poll_while_pending().and(parsed) {
            Ok((options, fetch)) => {
                let job = Job {
                    id,
                    source: source.to_string(),
                    options,
                    fetch,
                    rules: self.rules.clone(),
                    has_callback: self.http_headers_callback.is_valid(),
                    cache: (self.get_cache_max_bytes() > 0).then(|| self.cache()),
                    cancelled,
                };
                spawn(job, self.sender.clone());
            }
            // Reported like any failure, on the next frame (without a SceneTree nothing polls: say so here too).
            Err(e) => {
                if Engine::singleton().get_main_loop().and_then(|l| l.try_cast::<SceneTree>().ok()).is_none() {
                    godot_error!("shadewire: {e}");
                }
                let _ = self.sender.send(Event::Done { id, result: Err(e) });
            }
        }
        id
    }

    /// Stops the request: it ends with `material_failed(id, "cancelled")`. False for an unknown or finished id.
    #[func]
    fn cancel(&mut self, id: i64) -> bool {
        self.requests.get(&id).inspect(|c| c.store(true, Ordering::Relaxed)).is_some()
    }

    /// Headers for the hosts `host_pattern` names: `cdn.example.com`, `*.example.org` (subdomains), optionally with
    /// `:port`. Several patterns can apply; the most specific wins a name clash. An empty array removes the
    /// pattern. False on a malformed pattern or header (see `get_last_error`, which never shows values).
    #[func]
    fn set_http_headers(&mut self, host_pattern: GString, headers: PackedStringArray) -> bool {
        let lines: Vec<String> = headers.as_slice().iter().map(GString::to_string).collect();
        let result = Header::parse_all(lines.iter().map(String::as_str))
            .and_then(|headers| self.rules.set(&host_pattern.to_string(), headers));
        self.finish(result).is_some()
    }

    /// Removes every `set_http_headers` rule.
    #[func]
    fn clear_http_headers(&mut self) {
        self.rules.clear();
    }

    #[func]
    fn get_cache_max_bytes(&self) -> i64 {
        self.with_cache(|c| c.max_bytes()) as i64
    }

    #[func]
    fn set_cache_max_bytes(&mut self, max_bytes: i64) {
        self.with_cache(|c| c.set_max_bytes(max_bytes.max(0) as u64));
    }

    /// Bytes the disk cache holds.
    #[func]
    fn get_cache_size(&self) -> i64 {
        self.with_cache(|c| c.size()) as i64
    }

    /// Removes the cached copy of `url` (a document, include or texture); false when there was none.
    #[func]
    fn evict_cache(&mut self, url: GString) -> bool {
        let url = url.to_string();
        self.with_cache(|c| c.evict_key(without_fragment(&url)))
    }

    #[func]
    fn clear_cache(&mut self) {
        self.with_cache(cache::DiskCache::clear);
    }

    /// Drains the workers' events: answers header callbacks, emits progress, then finishes requests.
    #[func(gd_self)]
    fn _shadewire_poll(mut this: Gd<Self>) {
        let (events, callback) = {
            let s = this.bind();
            (s.receiver.try_iter().collect::<Vec<_>>(), s.http_headers_callback.clone())
        };
        let mut progress: Vec<(i64, u64, u64)> = Vec::new();
        let mut done = Vec::new();
        for event in events {
            match event {
                Event::Headers { url, reply } => {
                    let lines = if callback.is_valid() {
                        callback.callv(&varray![url.as_str()]).try_to::<PackedStringArray>().unwrap_or_default()
                    } else {
                        PackedStringArray::new()
                    };
                    let _ = reply.send(lines.as_slice().iter().map(GString::to_string).collect());
                }
                Event::Progress { id, done, total } => {
                    progress.retain(|p| p.0 != id);
                    progress.push((id, done, total));
                }
                Event::Done { id, result } => done.push((id, result)),
            }
        }
        for (id, received, total) in progress {
            if this.bind().requests.contains_key(&id) {
                this.signals().material_progress().emit(id, received as i64, total as i64);
            }
        }
        for (id, result) in done {
            let Some(cancelled) = this.bind_mut().requests.remove(&id) else { continue };
            let result = if cancelled.load(Ordering::Relaxed) {
                Err(CANCELLED.to_string())
            } else {
                result.and_then(finish_fetched)
            };
            match result {
                Ok(material) => this.signals().material_ready().emit(id, &material),
                Err(e) => this.signals().material_failed().emit(id, e.as_str()),
            }
        }
        let _ = this.bind_mut().poll_while_pending();
    }

    /// A document held in a string. `base` is where relative includes and textures resolve: a directory or a
    /// document location; empty makes them errors.
    #[func]
    fn compile_string(
        &mut self,
        text: GString,
        #[opt(default = "")] base: GString,
        #[opt(default = Options::default())] options: Options,
    ) -> Option<Gd<ShaderMaterial>> {
        let mut base = base.to_string();
        if !base.is_empty() && !base.ends_with(['/', '\\']) && DirAccess::dir_exists_absolute(&base) {
            base.push('/');
        }
        let location = (!base.is_empty()).then_some(base.as_str());
        let result = options_from(&options.0).and_then(|options| load_text(&text.to_string(), location, &options));
        self.finish(result)
    }

    /// The names of the document's materials, in document order; empty on error (see `get_last_error`).
    #[func]
    fn material_names(&mut self, path: GString) -> PackedStringArray {
        let path = path.to_string();
        let result = read_text(&path).and_then(|text| {
            shadewire_mtlx::material_names(&text, Some(&path), &resolver()).map_err(|e| e.to_string())
        });
        let names = self.finish(result).unwrap_or_default();
        names.iter().map(|n| GString::from(n.as_str())).collect()
    }

    /// The message of the last failed call; empty after a success.
    #[func]
    fn get_last_error(&self) -> GString {
        self.last_error.clone()
    }
}
