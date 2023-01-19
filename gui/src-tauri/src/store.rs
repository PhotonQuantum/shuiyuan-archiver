use std::fs;
use std::path::PathBuf;

use directories::ProjectDirs;

use crate::Result;

#[derive(Debug)]
pub struct Store {
    cache_dir: PathBuf,
}

impl Store {
    pub fn new() -> Result<Self> {
        let dir = ProjectDirs::from("me", "lightquantum", "shuiyuan-archiver").unwrap();
        let cache_dir = dir.cache_dir().to_owned();
        Ok(Self { cache_dir })
    }
    pub fn get_token(&self) -> Option<String> {
        fs::read_to_string(self.cache_dir.join("token")).ok()
    }
    pub fn delete_token(&self) {
        let _ = fs::remove_file(self.cache_dir.join("token"));
    }
    pub fn set_token(&self, token: &str) -> Result<()> {
        fs::create_dir_all(&self.cache_dir)?;
        fs::write(self.cache_dir.join("token"), token)?;
        Ok(())
    }
}
