//! The disk cache of fetched documents and textures: one body file per URL plus `index.json` (validators, size,
//! last access). The cache is capped in bytes, evicting the least recently used entries; entries an in-flight
//! request uses are kept until it ends. Keys are URLs without their fragment, never headers.

use std::collections::{BTreeMap, HashMap};
use std::path::{Path, PathBuf};
use std::sync::{Arc, LazyLock, Mutex};

use serde::{Deserialize, Serialize};

pub const DEFAULT_MAX_BYTES: u64 = 256 * 1024 * 1024;
const INDEX: &str = "index.json";

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Entry {
    /// The body file, in the cache directory.
    pub file: String,
    pub size: u64,
    pub etag: Option<String>,
    pub last_modified: Option<String>,
    /// The index's clock at the last read or write: the LRU order.
    pub last_access: u64,
}

/// Entries by key, and a logical clock for their access order. No I/O: `DiskCache` keeps the files in step.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Index {
    entries: BTreeMap<String, Entry>,
    clock: u64,
}

impl Index {
    pub fn get(&self, key: &str) -> Option<&Entry> {
        self.entries.get(key)
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    pub fn total(&self) -> u64 {
        self.entries.values().map(|e| e.size).sum()
    }

    pub fn touch(&mut self, key: &str) {
        self.clock += 1;
        if let Some(entry) = self.entries.get_mut(key) {
            entry.last_access = self.clock;
        }
    }

    /// Adds or replaces `key`, as just accessed. Returns the entries it displaced: the old entry of `key`, and
    /// another key's entry using the same file.
    pub fn insert(&mut self, key: &str, mut entry: Entry) -> Vec<Entry> {
        self.clock += 1;
        entry.last_access = self.clock;
        let clash: Vec<String> =
            self.entries.iter().filter(|(k, e)| *k != key && e.file == entry.file).map(|(k, _)| k.clone()).collect();
        let mut displaced: Vec<Entry> = clash.iter().filter_map(|k| self.entries.remove(k)).collect();
        displaced.extend(self.entries.insert(key.to_string(), entry));
        displaced
    }

    pub fn remove(&mut self, key: &str) -> Option<Entry> {
        self.entries.remove(key)
    }

    pub fn drain(&mut self) -> Vec<Entry> {
        std::mem::take(&mut self.entries).into_values().collect()
    }

    /// Removes least recently used entries, skipping `pinned` keys, until the total is at most `cap`.
    pub fn evict_to(&mut self, cap: u64, pinned: impl Fn(&str) -> bool) -> Vec<Entry> {
        let mut total = self.total();
        let mut order: Vec<(u64, String)> =
            self.entries.iter().filter(|(k, _)| !pinned(k)).map(|(k, e)| (e.last_access, k.clone())).collect();
        order.sort();
        let mut evicted = Vec::new();
        for (_, key) in order {
            if total <= cap {
                break;
            }
            let entry = self.entries.remove(&key).expect("listed above");
            total -= entry.size;
            evicted.push(entry);
        }
        evicted
    }

    /// Entries whose body file `exists` says is gone are dropped (a cache emptied by hand).
    pub fn retain_files(&mut self, exists: impl Fn(&str) -> bool) {
        self.entries.retain(|_, e| exists(&e.file));
    }
}

/// The body file of `key`: FNV-1a of the key (a clash with another key displaces it).
pub fn file_name(key: &str) -> String {
    let hash = key.bytes().fold(0xcbf2_9ce4_8422_2325u64, |h, b| (h ^ b as u64).wrapping_mul(0x0100_0000_01b3));
    format!("{hash:016x}.bin")
}

/// A cached body and its validators.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Cached {
    pub body: Vec<u8>,
    pub etag: Option<String>,
    pub last_modified: Option<String>,
}

/// The cache in one directory. Writes and evictions update `index.json` at once.
#[derive(Debug)]
pub struct DiskCache {
    dir: PathBuf,
    index: Index,
    max_bytes: u64,
    pinned: HashMap<String, usize>,
}

impl DiskCache {
    pub fn open(dir: &Path, max_bytes: u64) -> DiskCache {
        let mut index: Index = std::fs::read(dir.join(INDEX))
            .ok()
            .and_then(|bytes| serde_json::from_slice(&bytes).ok())
            .unwrap_or_default();
        index.retain_files(|file| dir.join(file).is_file());
        let mut cache = DiskCache { dir: dir.to_path_buf(), index, max_bytes, pinned: HashMap::new() };
        cache.evict();
        cache
    }

    pub fn max_bytes(&self) -> u64 {
        self.max_bytes
    }

    /// 0 disables the cache (and empties it).
    pub fn set_max_bytes(&mut self, max_bytes: u64) {
        self.max_bytes = max_bytes;
        self.evict();
    }

    pub fn size(&self) -> u64 {
        self.index.total()
    }

    pub fn len(&self) -> usize {
        self.index.len()
    }

    pub fn is_empty(&self) -> bool {
        self.index.is_empty()
    }

    /// The body and validators of `key`, as just used; a missing or unreadable body drops the entry.
    pub fn get(&mut self, key: &str) -> Option<Cached> {
        let entry = self.index.get(key)?.clone();
        match std::fs::read(self.dir.join(&entry.file)) {
            Ok(body) if body.len() as u64 == entry.size => {
                self.index.touch(key);
                self.save();
                Some(Cached { body, etag: entry.etag, last_modified: entry.last_modified })
            }
            _ => {
                self.evict_key(key);
                None
            }
        }
    }

    /// Stores `body` for `key` unless the cache is off or the body alone exceeds the cap, then evicts.
    pub fn put(&mut self, key: &str, body: &[u8], etag: Option<String>, last_modified: Option<String>) {
        let size = body.len() as u64;
        if size > self.max_bytes {
            self.evict_key(key);
            return;
        }
        let file = file_name(key);
        if std::fs::create_dir_all(&self.dir).is_err() {
            return;
        }
        let temp = self.dir.join(format!("{file}.tmp"));
        if std::fs::write(&temp, body).is_err() || std::fs::rename(&temp, self.dir.join(&file)).is_err() {
            let _ = std::fs::remove_file(&temp);
            return;
        }
        let entry = Entry { file: file.clone(), size, etag, last_modified, last_access: 0 };
        for old in self.index.insert(key, entry) {
            if old.file != file {
                let _ = std::fs::remove_file(self.dir.join(&old.file));
            }
        }
        self.evict();
    }

    pub fn evict_key(&mut self, key: &str) -> bool {
        let Some(entry) = self.index.remove(key) else { return false };
        let _ = std::fs::remove_file(self.dir.join(entry.file));
        self.save();
        true
    }

    /// Everything goes, pinned entries included (their requests already hold the bodies).
    pub fn clear(&mut self) {
        for entry in self.index.drain() {
            let _ = std::fs::remove_file(self.dir.join(entry.file));
        }
        self.save();
    }

    pub fn pin(&mut self, key: &str) {
        *self.pinned.entry(key.to_string()).or_default() += 1;
    }

    pub fn unpin(&mut self, key: &str) {
        if let Some(count) = self.pinned.get_mut(key) {
            *count -= 1;
            if *count == 0 {
                self.pinned.remove(key);
                self.evict();
            }
        }
    }

    fn evict(&mut self) {
        let pinned = &self.pinned;
        for entry in self.index.evict_to(self.max_bytes, |k| pinned.contains_key(k)) {
            let _ = std::fs::remove_file(self.dir.join(entry.file));
        }
        self.save();
    }

    fn save(&self) {
        if self.index.is_empty() && !self.dir.join(INDEX).exists() {
            return;
        }
        if std::fs::create_dir_all(&self.dir).is_ok()
            && let Ok(json) = serde_json::to_vec(&self.index)
        {
            let temp = self.dir.join(format!("{INDEX}.tmp"));
            if std::fs::write(&temp, json).is_ok() {
                let _ = std::fs::rename(&temp, self.dir.join(INDEX));
            }
        }
    }
}

pub type SharedCache = Arc<Mutex<DiskCache>>;

/// One cache per directory for the whole process, so `Shadewire` objects sharing a directory share its index.
/// `max_bytes` applies when the directory is first opened.
pub fn shared(dir: &Path, max_bytes: u64) -> SharedCache {
    static CACHES: LazyLock<Mutex<HashMap<PathBuf, SharedCache>>> = LazyLock::new(Default::default);
    let mut caches = CACHES.lock().unwrap_or_else(|e| e.into_inner());
    caches.entry(dir.to_path_buf()).or_insert_with(|| Arc::new(Mutex::new(DiskCache::open(dir, max_bytes)))).clone()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(file: &str, size: u64) -> Entry {
        Entry { file: file.into(), size, etag: None, last_modified: None, last_access: 0 }
    }

    #[test]
    fn eviction_drops_the_least_recently_used_first() {
        let mut index = Index::default();
        index.insert("a", entry("a", 10));
        index.insert("b", entry("b", 10));
        index.insert("c", entry("c", 10));
        index.touch("a");
        let evicted = index.evict_to(15, |_| false);
        assert_eq!(evicted.iter().map(|e| e.file.as_str()).collect::<Vec<_>>(), ["b", "c"]);
        assert_eq!((index.total(), index.get("a").is_some()), (10, true));
    }

    #[test]
    fn pinned_entries_survive_eviction() {
        let mut index = Index::default();
        index.insert("old", entry("old", 10));
        index.insert("new", entry("new", 10));
        let evicted = index.evict_to(0, |k| k == "old");
        assert_eq!(evicted, [Entry { last_access: 2, ..entry("new", 10) }]);
        assert_eq!(index.total(), 10);
    }

    #[test]
    fn inserting_replaces_and_reports_what_it_displaced() {
        let mut index = Index::default();
        assert!(index.insert("k", entry("f1", 5)).is_empty());
        let displaced = index.insert("k", entry("f2", 7));
        assert_eq!(displaced.iter().map(|e| e.file.as_str()).collect::<Vec<_>>(), ["f1"]);
        // Another key on the same file (a hash clash) is displaced too.
        let displaced = index.insert("j", entry("f2", 3));
        assert_eq!(displaced.iter().map(|e| e.file.as_str()).collect::<Vec<_>>(), ["f2"]);
        assert_eq!((index.len(), index.total()), (1, 3));
    }

    #[test]
    fn the_index_round_trips_through_json() {
        let mut index = Index::default();
        index.insert("https://x.org/a.png", Entry { etag: Some("\"e\"".into()), ..entry("f", 3) });
        let json = serde_json::to_string(&index).unwrap();
        assert_eq!(serde_json::from_str::<Index>(&json).unwrap(), index);
    }

    #[test]
    fn file_names_are_stable_hashes() {
        assert_eq!(file_name("https://x.org/a.png"), file_name("https://x.org/a.png"));
        assert_ne!(file_name("https://x.org/a.png"), file_name("https://x.org/b.png"));
        assert!(file_name("k").ends_with(".bin") && file_name("k").len() == 20);
    }

    fn temp_dir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("shadewire-cache-test-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        dir
    }

    #[test]
    fn the_disk_cache_keeps_bodies_and_reopens_with_them() {
        let dir = temp_dir("reopen");
        let mut cache = DiskCache::open(&dir, 100);
        cache.put("a", b"hello", Some("\"1\"".into()), None);
        assert_eq!(cache.get("a").unwrap().etag.as_deref(), Some("\"1\""));
        drop(cache);
        let mut cache = DiskCache::open(&dir, 100);
        assert_eq!((cache.size(), cache.get("a").unwrap().body), (5, b"hello".to_vec()));
        // A body file removed by hand drops its entry.
        std::fs::remove_file(dir.join(file_name("a"))).unwrap();
        assert!(cache.get("a").is_none() && cache.is_empty());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn lowering_the_cap_evicts_down_to_it() {
        let dir = temp_dir("cap");
        let mut cache = DiskCache::open(&dir, 100);
        for key in ["a", "b", "c", "d"] {
            cache.put(key, &[0; 20], None, None);
        }
        cache.get("a");
        cache.set_max_bytes(45);
        assert_eq!(cache.size(), 40);
        assert!(cache.get("a").is_some() && cache.get("b").is_none() && cache.get("d").is_some());
        assert!(!dir.join(file_name("b")).exists());
        // Too big for the cap: not stored. A cap of 0 turns the cache off.
        cache.put("big", &[0; 50], None, None);
        assert!(cache.get("big").is_none());
        cache.set_max_bytes(0);
        assert_eq!((cache.size(), cache.len()), (0, 0));
        cache.put("e", b"x", None, None);
        assert!(cache.is_empty());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn pinned_entries_wait_for_their_request() {
        let dir = temp_dir("pin");
        let mut cache = DiskCache::open(&dir, 100);
        cache.put("a", &[0; 30], None, None);
        cache.pin("a");
        cache.set_max_bytes(10);
        assert_eq!(cache.size(), 30);
        cache.unpin("a");
        assert_eq!(cache.size(), 0);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
