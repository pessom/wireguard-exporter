//! Friendly peer names from a YAML file (`--wireguard.peer-names-file`).
//!
//! ```yaml
//! "xTIBA5rboUvnH4htodjb6e697QjLERt1NAB4mZqp8Dg=": alice-laptop
//! "HIgo9xNzJMWLKASShiTqIybxZ0U3wGLiUeJ1PKf8ykw=":
//!   name: office-gateway
//!   site: fra          # extra labels
//! ```
//!
//! The file is checked on every scrape and reloaded when its modification
//! time or size changes. A broken file at startup is a fatal error; a broken
//! file at runtime is logged and the previous names are kept.

use std::collections::{BTreeMap, HashMap};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::SystemTime;

use anyhow::{Context, bail};
use serde::Deserialize;

/// Labels that the exporter or Prometheus set themselves.
const RESERVED_LABELS: &[&str] = &["interface", "public_key", "name", "job", "instance"];

/// Name and extra labels of one peer; labels are sorted by name.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PeerMeta {
    pub name: String,
    pub labels: Vec<(String, String)>,
}

/// Public key (base64) -> peer metadata.
pub type NameMap = HashMap<String, PeerMeta>;

#[derive(Deserialize)]
#[serde(untagged)]
enum Entry {
    Name(String),
    Full {
        name: String,
        #[serde(flatten)]
        labels: BTreeMap<String, String>,
    },
}

/// Parses and validates the file content.
pub fn parse(content: &str) -> anyhow::Result<NameMap> {
    // An empty file (or only comments) is a valid empty mapping.
    if content.lines().all(|l| {
        let l = l.trim();
        l.is_empty() || l.starts_with('#')
    }) {
        return Ok(NameMap::new());
    }
    let raw: BTreeMap<String, Entry> = serde_saphyr::from_str(content)
        .context("invalid YAML: expected a map of public key -> name or {name, labels...}")?;

    let mut map = NameMap::with_capacity(raw.len());
    for (key, entry) in raw {
        if !is_public_key(&key) {
            bail!("{key:?}: not a WireGuard public key (44 base64 characters ending with '=')");
        }
        let (name, labels) = match entry {
            Entry::Name(name) => (name, BTreeMap::new()),
            Entry::Full { name, labels } => (name, labels),
        };
        if name.trim().is_empty() {
            bail!("{key}: empty name");
        }
        for label in labels.keys() {
            if !is_label_name(label) {
                bail!("{key}: invalid label name {label:?}");
            }
            if RESERVED_LABELS.contains(&label.as_str()) || label.starts_with("__") {
                bail!("{key}: label {label:?} is reserved");
            }
        }
        map.insert(
            key,
            PeerMeta {
                name,
                labels: labels.into_iter().collect(),
            },
        );
    }
    Ok(map)
}

fn is_public_key(s: &str) -> bool {
    s.len() == 44
        && s.ends_with('=')
        && s[..43]
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'+' || b == b'/')
}

/// Prometheus label name: `[a-zA-Z_][a-zA-Z0-9_]*`.
fn is_label_name(s: &str) -> bool {
    let mut bytes = s.bytes();
    matches!(bytes.next(), Some(b) if b.is_ascii_alphabetic() || b == b'_')
        && bytes.all(|b| b.is_ascii_alphanumeric() || b == b'_')
}

/// Identity of a file version: modification time and size.
type Stamp = (Option<SystemTime>, u64);

fn stamp(path: &Path) -> std::io::Result<Stamp> {
    // Follows symlinks: Kubernetes ConfigMap updates swap a symlink.
    let meta = std::fs::metadata(path)?;
    Ok((meta.modified().ok(), meta.len()))
}

struct Loaded {
    stamp: Stamp,
    names: Arc<NameMap>,
    ok: bool,
}

/// Peer names file with reload on change.
pub struct PeerNames {
    path: PathBuf,
    state: Mutex<Loaded>,
}

/// Names for one scrape and whether the last (re)load succeeded.
pub struct Snapshot {
    pub names: Arc<NameMap>,
    pub ok: bool,
}

impl PeerNames {
    /// Loads the file; errors are fatal at startup.
    pub fn load(path: PathBuf) -> anyhow::Result<Self> {
        let stamp = stamp(&path).with_context(|| format!("reading {}", path.display()))?;
        let content = std::fs::read_to_string(&path)
            .with_context(|| format!("reading {}", path.display()))?;
        let names = parse(&content).with_context(|| format!("parsing {}", path.display()))?;
        tracing::info!(path = %path.display(), peers = names.len(), "peer names loaded");
        Ok(Self {
            path,
            state: Mutex::new(Loaded {
                stamp,
                names: Arc::new(names),
                ok: true,
            }),
        })
    }

    /// Returns current names, reloading the file first if it changed.
    pub fn snapshot(&self) -> Snapshot {
        let mut state = self.state.lock().unwrap_or_else(|e| e.into_inner());
        match stamp(&self.path) {
            Ok(current) if current == state.stamp => {}
            Ok(current) => {
                match std::fs::read_to_string(&self.path)
                    .map_err(anyhow::Error::from)
                    .and_then(|c| parse(&c))
                {
                    Ok(names) => {
                        tracing::info!(path = %self.path.display(), peers = names.len(), "peer names reloaded");
                        state.names = Arc::new(names);
                        state.ok = true;
                    }
                    Err(err) => {
                        tracing::warn!(path = %self.path.display(), error = format!("{err:#}"), "peer names reload failed, keeping previous names");
                        state.ok = false;
                    }
                }
                // Do not retry an unchanged broken file on every scrape.
                state.stamp = current;
            }
            Err(err) => {
                if state.ok {
                    tracing::warn!(path = %self.path.display(), error = %err, "peer names file unavailable, keeping previous names");
                }
                state.ok = false;
            }
        }
        Snapshot {
            names: Arc::clone(&state.names),
            ok: state.ok,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const A: &str = "xTIBA5rboUvnH4htodjb6e697QjLERt1NAB4mZqp8Dg=";
    const B: &str = "HIgo9xNzJMWLKASShiTqIybxZ0U3wGLiUeJ1PKf8ykw=";

    fn tmp(tag: &str) -> PathBuf {
        std::env::temp_dir().join(format!("wgexp-names-{tag}-{}.yml", std::process::id()))
    }

    #[test]
    fn short_and_full_forms() {
        let map = parse(&format!(
            "\"{A}\": alice-laptop\n\"{B}\":\n  name: office-gw\n  site: fra\n  owner: it\n"
        ))
        .unwrap();
        assert_eq!(
            map[A],
            PeerMeta {
                name: "alice-laptop".into(),
                labels: vec![]
            }
        );
        assert_eq!(
            map[B],
            PeerMeta {
                name: "office-gw".into(),
                labels: vec![("owner".into(), "it".into()), ("site".into(), "fra".into())],
            }
        );
    }

    #[test]
    fn unquoted_key_and_comments() {
        let map = parse(&format!("# peers\n{A}: alice # laptop\n")).unwrap();
        assert_eq!(map[A].name, "alice");
    }

    #[test]
    fn unquoted_keys_with_plus_and_slash() {
        // Helm's toYaml writes keys without quotes.
        let k1 = "+TIBA5rboUvnH4htodjb6e697QjLERt1NAB4mZqp8Dg=";
        let k2 = "/Igo9xNzJMWLKASShiTqIybxZ0U3wGLiUeJ1PKf8yk+=";
        let map = parse(&format!("{k1}: a\n{k2}:\n  name: b\n")).unwrap();
        assert_eq!(map[k1].name, "a");
        assert_eq!(map[k2].name, "b");
    }

    #[test]
    fn empty_file_is_empty_map() {
        assert!(parse("").unwrap().is_empty());
        assert!(parse("# nothing yet\n\n").unwrap().is_empty());
    }

    #[test]
    fn rejects_bad_key() {
        let err = parse("not-a-key: alice\n").unwrap_err().to_string();
        assert!(err.contains("not a WireGuard public key"), "{err}");
    }

    #[test]
    fn rejects_empty_name() {
        assert!(parse(&format!("\"{A}\": \"\"\n")).is_err());
    }

    #[test]
    fn rejects_reserved_and_invalid_labels() {
        for label in [
            "interface",
            "public_key",
            "instance",
            "__meta",
            "1site",
            "si-te",
        ] {
            let yaml = format!("\"{A}\":\n  name: a\n  \"{label}\": x\n");
            assert!(parse(&yaml).is_err(), "label {label} must be rejected");
        }
    }

    #[test]
    fn rejects_non_map() {
        assert!(parse("- a\n- b\n").is_err());
    }

    #[test]
    fn load_fails_on_missing_or_invalid_file() {
        assert!(PeerNames::load(tmp("missing")).is_err());
        let path = tmp("invalid");
        std::fs::write(&path, "bad: [").unwrap();
        assert!(PeerNames::load(path.clone()).is_err());
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn reloads_on_change_and_keeps_names_on_error() {
        let path = tmp("reload");
        std::fs::write(&path, format!("\"{A}\": alice\n")).unwrap();
        let names = PeerNames::load(path.clone()).unwrap();
        assert_eq!(names.snapshot().names[A].name, "alice");

        // Different size guarantees a new stamp even with coarse mtime.
        std::fs::write(&path, format!("\"{A}\": alice-renamed\n")).unwrap();
        let snap = names.snapshot();
        assert!(snap.ok);
        assert_eq!(snap.names[A].name, "alice-renamed");

        std::fs::write(&path, "broken: [").unwrap();
        let snap = names.snapshot();
        assert!(!snap.ok);
        assert_eq!(snap.names[A].name, "alice-renamed");

        std::fs::remove_file(&path).unwrap();
        let snap = names.snapshot();
        assert!(!snap.ok);
        assert_eq!(snap.names[A].name, "alice-renamed");
    }
}
