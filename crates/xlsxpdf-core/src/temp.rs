use crate::error::{Error, Result};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

pub struct TempStore {
    root: PathBuf,
    quota: u64,
    used: AtomicU64,
}

impl TempStore {
    pub fn new(root: &Path, quota: u64) -> Result<Self> {
        fs::create_dir_all(root)?;
        Ok(Self {
            root: root.to_path_buf(),
            quota,
            used: AtomicU64::new(0),
        })
    }

    pub fn create_file(&self, name: &str) -> Result<PathBuf> {
        let path = self.root.join(name);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        Ok(path)
    }

    pub fn account(&self, bytes: u64) -> Result<()> {
        let prev = self.used.fetch_add(bytes, Ordering::Relaxed);
        if prev + bytes > self.quota {
            self.used.fetch_sub(bytes, Ordering::Relaxed);
            return Err(Error::ResourceLimit(format!(
                "temp storage quota exceeded: used={prev}, requested={bytes}, quota={}",
                self.quota
            )));
        }
        Ok(())
    }

    pub fn release(&self, bytes: u64) {
        self.used.fetch_sub(bytes, Ordering::Relaxed);
    }

    pub fn used(&self) -> u64 {
        self.used.load(Ordering::Relaxed)
    }

    pub fn root(&self) -> &Path {
        &self.root
    }
}

impl Drop for TempStore {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
    }
}
