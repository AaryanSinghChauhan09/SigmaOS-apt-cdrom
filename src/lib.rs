//! SigmaOS-apt-cdrom — Debian CD-ROM/APT source management.
//!
//! Inspired by Debian's `apt-cdrom`: manages `cdrom:` entries in
//! `sources.list` — identifying a disc (by `.disk/info` `DISK_LABEL` /
//! `DISK_TITLE`), validating it carries a usable `dists/` tree, and emitting
//! the correct one-line `deb cdrom:[label]/ distro component(s)` source.

#![forbid(unsafe_code)]
#![warn(missing_docs)]

use std::fmt;
use std::fs;
use std::path::{Path, PathBuf};

/// Errors raised by this component.
#[derive(Debug, thiserror::Error)]
pub enum ComponentError {
    /// The path is not a usable APT disc (missing `.disk/info` or `dists/`).
    #[error("not an apt disc: {0}")]
    NotDisc(PathBuf),
    /// `.disk/info` existed but had no `DISK_LABEL`/`DISK_TITLE`.
    #[error("disc missing DISK_LABEL/DISK_TITLE: {0}")]
    NoLabel(PathBuf),
    /// Underlying I/O failure.
    #[error("i/o error: {0}")]
    Io(#[from] std::io::Error),
}

/// Result alias for component operations.
pub type ComponentResult<T> = Result<T, ComponentError>;

/// A recognised APT CD-ROM source.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CdromSource {
    /// The `cdrom:[label]` token used in sources.list.
    pub label: String,
    /// Filesystem path the disc is mounted at.
    pub mount: PathBuf,
    /// Suite/distribution directory under `dists/` (e.g. `stable`).
    pub suite: String,
    /// Components (e.g. `main`, `contrib`, `non-free`).
    pub components: Vec<String>,
}

impl CdromSource {
    /// Probe `path` as an APT disc: requires `.disk/info` with a label and a
    /// `dists/` directory. Returns the identified source (suite/components
    /// default to `stable`/`main` and can be overridden).
    pub fn probe(path: impl AsRef<Path>, suite: &str, components: &[&str]) -> ComponentResult<Self> {
        let root = path.as_ref();
        let info = root.join(".disk").join("info");
        if !info.is_file() || !root.join("dists").is_dir() {
            return Err(ComponentError::NotDisc(root.to_path_buf()));
        }
        let text = fs::read_to_string(&info)?;
        let mut label = None;
        let mut title = None;
        for line in text.lines() {
            if let Some(v) = line.strip_prefix("DISK_LABEL=") { label = Some(v.trim().to_string()); }
            if let Some(v) = line.strip_prefix("DISK_TITLE=") { title = Some(v.trim().to_string()); }
        }
        // Prefer the more descriptive title, else label.
        let name = title.or(label).ok_or_else(|| ComponentError::NoLabel(root.to_path_buf()))?;
        Ok(Self {
            label: name,
            mount: root.to_path_buf(),
            suite: suite.to_string(),
            components: components.iter().map(|s| s.to_string()).collect(),
        })
    }

    /// The `deb cdrom:[...]` sources.list line for this disc.
    pub fn sources_line(&self) -> String {
        let comps = if self.components.is_empty() {
            "main".to_string()
        } else {
            self.components.join(" ")
        };
        format!("deb cdrom:[{}]/ {} {}", self.label, self.suite, comps)
    }

    /// Distributions present under `dists/`.
    pub fn available_suites(&self) -> ComponentResult<Vec<String>> {
        let dists = self.mount.join("dists");
        let mut out = Vec::new();
        for e in fs::read_dir(&dists)? {
            let e = e?;
            if e.file_type()?.is_dir() {
                out.push(e.file_name().to_string_lossy().into_owned());
            }
        }
        out.sort();
        Ok(out)
    }
}

impl fmt::Display for CdromSource {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.sources_line())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_disc(root: &Path, label: &str) -> PathBuf {
        let disk = root.join(".disk");
        fs::create_dir_all(&disk).unwrap();
        fs::write(disk.join("info"), format!("DISK_LABEL={}\nDISK_TITLE={}\n", label, label)).unwrap();
        fs::create_dir_all(root.join("dists").join("stable")).unwrap();
        fs::create_dir_all(root.join("dists").join("testing")).unwrap();
        root.to_path_buf()
    }

    #[test]
    fn probe_identifies_disc() {
        let dir = std::env::temp_dir().join(format!("cdrom-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        let mnt = make_disc(&dir, "Debian 12.5 amd64");
        let src = CdromSource::probe(&mnt, "stable", &["main", "contrib"]).unwrap();
        assert_eq!(src.label, "Debian 12.5 amd64");
        assert_eq!(src.suite, "stable");
        assert_eq!(src.components, vec!["main", "contrib"]);
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn sources_line_format() {
        let src = CdromSource {
            label: "Debian 12".into(), mount: PathBuf::from("/media/cdrom"),
            suite: "stable".into(), components: vec!["main".into(), "non-free".into()],
        };
        assert_eq!(src.sources_line(), "deb cdrom:[Debian 12]/ stable main non-free");
    }

    #[test]
    fn rejects_non_disc() {
        let dir = std::env::temp_dir().join(format!("notdisc-{}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        assert!(matches!(CdromSource::probe(&dir, "stable", &["main"]), Err(ComponentError::NotDisc(_))));
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn lists_available_suites() {
        let dir = std::env::temp_dir().join(format!("cdrom2-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        let mnt = make_disc(&dir, "X");
        let src = CdromSource::probe(&mnt, "stable", &["main"]).unwrap();
        let suites = src.available_suites().unwrap();
        assert_eq!(suites, vec!["stable", "testing"]);
        let _ = fs::remove_dir_all(&dir);
    }
}
