//! Sharing a recording: upload it to the person's own S3-compatible storage
//! (Hetzner Object Storage and the like) and copy the link. The secret key
//! is kept in the Keychain; the rest is in Capture's settings.

pub mod s3;

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::Ordering;
use std::sync::mpsc::{channel, Receiver};
use std::sync::Arc;

/// What a link does.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum LinkKind {
    /// Anyone with the link, for good (the file is made public-read; its
    /// name is random, so it can't be guessed).
    Public,
    /// The bucket stays private; the link stops working after 7 days.
    Week,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    /// Upload every recording as soon as it's saved.
    pub auto: bool,
    /// Put the link on the clipboard when the upload is done.
    pub copy_link: bool,
    /// "fsn1.your-objectstorage.com".
    pub endpoint: String,
    /// Empty: taken from the endpoint.
    pub region: String,
    pub bucket: String,
    pub access_key: String,
    /// Folder inside the bucket ("recordings/"); may be empty.
    pub folder: String,
    pub link: LinkKind,
}

impl Default for Settings {
    fn default() -> Self {
        Settings { auto: false, copy_link: true, endpoint: String::new(), region: String::new(), bucket: String::new(), access_key: String::new(), folder: "recordings/".into(), link: LinkKind::Public }
    }
}

impl Settings {
    pub fn bucket(&self) -> s3::Bucket {
        s3::Bucket::new(&self.endpoint, &self.region, &self.bucket, &self.access_key, &secret::get().unwrap_or_default())
    }
    /// Everything filled in (the secret included).
    pub fn ready(&self) -> bool {
        self.bucket().ready()
    }
}

/// The secret key, in the Keychain (read once, then kept in memory).
pub mod secret {
    use std::sync::Mutex;

    static CACHE: Mutex<Option<Option<String>>> = Mutex::new(None);

    pub fn get() -> Option<String> {
        let mut c = CACHE.lock().unwrap_or_else(|e| e.into_inner());
        c.get_or_insert_with(keychain::get).clone()
    }

    /// Save it (empty removes it).
    pub fn set(v: &str) -> Result<(), String> {
        keychain::set(v)?;
        *CACHE.lock().unwrap_or_else(|e| e.into_inner()) = Some(Some(v.to_string()).filter(|s| !s.is_empty()));
        Ok(())
    }

    #[cfg(target_os = "macos")]
    mod keychain {
        const SERVICE: &str = "Cmd+Meow sharing";
        /// Where the key lived under the app's first name; moved over on first read.
        const OLD_SERVICE: &str = "CleanMeBro sharing";
        const ACCOUNT: &str = "secret-key";

        fn read(service: &str) -> Option<String> {
            security_framework::passwords::get_generic_password(service, ACCOUNT).ok().and_then(|b| String::from_utf8(b).ok()).filter(|s| !s.is_empty())
        }
        pub fn get() -> Option<String> {
            read(SERVICE).or_else(|| {
                let v = read(OLD_SERVICE)?;
                if set(&v).is_ok() {
                    let _ = security_framework::passwords::delete_generic_password(OLD_SERVICE, ACCOUNT);
                }
                Some(v)
            })
        }
        pub fn set(v: &str) -> Result<(), String> {
            if v.is_empty() {
                let _ = security_framework::passwords::delete_generic_password(SERVICE, ACCOUNT);
                return Ok(());
            }
            security_framework::passwords::set_generic_password(SERVICE, ACCOUNT, v.as_bytes()).map_err(|e| e.to_string())
        }
    }

    /// Elsewhere (tests): only in memory.
    #[cfg(not(target_os = "macos"))]
    mod keychain {
        pub fn get() -> Option<String> {
            None
        }
        pub fn set(_v: &str) -> Result<(), String> {
            Ok(())
        }
    }
}

/// A file that was shared: where it went and its last link.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Shared {
    pub key: String,
    pub link: String,
}

fn store_path() -> PathBuf {
    crate::kit::fs::data_path("shared.json")
}

fn load_store() -> HashMap<String, Shared> {
    crate::kit::fs::load_json(&store_path())
}

/// The last link of `path`, if it was shared.
pub fn link_for(path: &Path) -> Option<String> {
    load_store().get(&path.display().to_string()).map(|s| s.link.clone())
}

fn remember(path: &Path, s: Shared) {
    let mut all = load_store();
    all.insert(path.display().to_string(), s);
    crate::kit::fs::save_json(&store_path(), &all, true);
}

fn content_type(path: &Path) -> &'static str {
    match path.extension().map(|e| e.to_string_lossy().to_lowercase()).as_deref() {
        Some("mp4") | Some("m4v") => "video/mp4",
        Some("mov") => "video/quicktime",
        Some("png") => "image/png",
        Some("jpg") | Some("jpeg") => "image/jpeg",
        Some("gif") => "image/gif",
        _ => "application/octet-stream",
    }
}

/// A new random object name in `folder`, keeping the file's extension.
fn new_key(folder: &str, path: &Path) -> String {
    let mut b = [0u8; 16];
    if getrandom::fill(&mut b).is_err() {
        let t = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_nanos()).unwrap_or(0);
        b = t.to_le_bytes();
    }
    let id: String = b.iter().map(|x| format!("{x:02x}")).collect();
    let ext = path.extension().map(|e| format!(".{}", e.to_string_lossy().to_lowercase())).unwrap_or_default();
    let folder = folder.trim().trim_start_matches('/');
    let folder = if folder.is_empty() || folder.ends_with('/') { folder.to_string() } else { format!("{folder}/") };
    format!("{folder}{id}{ext}")
}

struct Job {
    path: PathBuf,
    progress: Arc<s3::Progress>,
    rx: Receiver<Result<String, String>>,
}

/// Uploads under way.
#[derive(Default)]
pub struct Uploads {
    jobs: Vec<Job>,
}

impl Uploads {
    /// Upload `path` (again: the same name, so a link to it keeps working).
    pub fn start(&mut self, s: &Settings, path: PathBuf, ctx: &egui::Context) -> Result<(), String> {
        let b = s.bucket();
        if !b.ready() {
            return Err("Set up sharing in Capture settings first".into());
        }
        self.stop(&path);
        let key = load_store().get(&path.display().to_string()).map(|x| x.key.clone()).unwrap_or_else(|| new_key(&s.folder, &path));
        let progress = Arc::new(s3::Progress::default());
        let (tx, rx) = channel();
        let (p2, ctx2, path2, link) = (progress.clone(), ctx.clone(), path.clone(), s.link);
        std::thread::Builder::new()
            .name("share-upload".into())
            .spawn(move || {
                let r = s3::put_file(&b, &key, &path2, content_type(&path2), link == LinkKind::Public, &p2).map(|()| {
                    let url = match link {
                        LinkKind::Public => b.url(&key),
                        LinkKind::Week => s3::presign_get(&b, &key, 7 * 24 * 3600, chrono::Utc::now()),
                    };
                    remember(&path2, Shared { key, link: url.clone() });
                    url
                });
                let _ = tx.send(r);
                ctx2.request_repaint();
            })
            .map_err(|e| e.to_string())?;
        self.jobs.push(Job { path, progress, rx });
        Ok(())
    }

    /// How far the upload of `path` is (0..1), while it runs.
    pub fn progress(&self, path: &Path) -> Option<f32> {
        self.jobs.iter().find(|j| j.path == path).map(|j| j.progress.fraction())
    }

    pub fn busy(&self) -> bool {
        !self.jobs.is_empty()
    }

    pub fn stop(&mut self, path: &Path) {
        self.jobs.retain(|j| {
            let same = j.path == path;
            if same {
                j.progress.cancel.store(true, Ordering::Relaxed);
            }
            !same
        });
    }

    /// Finished uploads: the file and its link (or why it failed).
    pub fn poll(&mut self) -> Vec<(PathBuf, Result<String, String>)> {
        let mut done = Vec::new();
        self.jobs.retain(|j| match j.rx.try_recv() {
            Ok(r) => {
                done.push((j.path.clone(), r));
                false
            }
            Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                done.push((j.path.clone(), Err("The upload stopped".into())));
                false
            }
            Err(_) => true,
        });
        done
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keys_are_random_and_keep_the_extension() {
        let a = new_key("recordings", Path::new("/x/Recording 1.MP4"));
        let b = new_key("/recordings/", Path::new("/x/Recording 1.mp4"));
        assert!(a.starts_with("recordings/") && a.ends_with(".mp4") && a.len() == "recordings/".len() + 32 + 4, "{a}");
        assert_ne!(a, b);
        assert!(new_key("", Path::new("a.png")).ends_with(".png"));
        assert_eq!(content_type(Path::new("a.mp4")), "video/mp4");
    }
}
