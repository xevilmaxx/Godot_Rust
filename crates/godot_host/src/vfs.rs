//! Port of `core/io/file_access.h` + `core/io/dir_access.h` (local-filesystem
//! subset). `res://` maps to the project dir, `user://` to a per-user data dir.

use godot_core::{GodotError, GodotResult};
use std::path::PathBuf;

fn project_dir() -> PathBuf {
    std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."))
}
fn user_dir() -> PathBuf {
    std::env::temp_dir().join("godot_rs_user")
}

/// Translate `res://` / `user://` to a real path.
pub fn translate(path: &str) -> PathBuf {
    if let Some(rest) = path.strip_prefix("res://") {
        project_dir().join(rest)
    } else if let Some(rest) = path.strip_prefix("user://") {
        user_dir().join(rest)
    } else {
        PathBuf::from(path)
    }
}

/// Mirrors `FileAccess` (read/write/seek/get_line).
pub struct FileAccess {
    data: Vec<u8>,
    pos: usize,
    write_path: Option<PathBuf>,
    write_mode: bool,
}

impl FileAccess {
    pub fn open_read(path: &str) -> GodotResult<Self> {
        let real = translate(path);
        let data = std::fs::read(&real).map_err(|e| GodotError::Io(format!("{path}: {e}")))?;
        Ok(Self { data, pos: 0, write_path: None, write_mode: false })
    }
    pub fn open_write(path: &str) -> GodotResult<Self> {
        let real = translate(path);
        if let Some(parent) = real.parent() {
            std::fs::create_dir_all(parent).map_err(|e| GodotError::Io(e.to_string()))?;
        }
        Ok(Self { data: vec![], pos: 0, write_path: Some(real), write_mode: true })
    }
    pub fn get_len(&self) -> usize {
        self.data.len()
    }
    pub fn get_position(&self) -> usize {
        self.pos
    }
    pub fn eof(&self) -> bool {
        self.pos >= self.data.len()
    }
    pub fn seek(&mut self, pos: usize) {
        self.pos = pos.min(self.data.len());
    }
    pub fn read_bytes(&mut self, n: usize) -> Vec<u8> {
        let end = (self.pos + n).min(self.data.len());
        let out = self.data[self.pos..end].to_vec();
        self.pos = end;
        out
    }
    pub fn read_text(&mut self) -> String {
        let out = String::from_utf8_lossy(&self.data[self.pos..]).into_owned();
        self.pos = self.data.len();
        out
    }
    pub fn get_line(&mut self) -> Option<String> {
        if self.eof() {
            return None;
        }
        let rest = &self.data[self.pos..];
        let end = rest.iter().position(|&b| b == b'\n').map(|i| self.pos + i).unwrap_or(self.data.len());
        let line = String::from_utf8_lossy(&self.data[self.pos..end]).trim_end_matches('\r').to_owned();
        self.pos = (end + 1).min(self.data.len());
        Some(line)
    }
    pub fn write_bytes(&mut self, bytes: &[u8]) {
        if self.write_mode {
            self.data.extend_from_slice(bytes);
            self.pos = self.data.len();
        }
    }
    pub fn write_text(&mut self, text: &str) {
        self.write_bytes(text.as_bytes());
    }
    /// Flush a write-mode file to disk (RAII would also work; explicit is clearer).
    pub fn close(self) -> GodotResult<()> {
        if let (true, Some(path)) = (self.write_mode, self.write_path) {
            std::fs::write(&path, &self.data).map_err(|e| GodotError::Io(e.to_string()))?;
        }
        Ok(())
    }
    pub fn exists(path: &str) -> bool {
        translate(path).exists()
    }
}

/// Mirrors `DirAccess` (list/mkdir/exists).
pub struct DirAccess;

impl DirAccess {
    pub fn exists(path: &str) -> bool {
        translate(path).is_dir()
    }
    pub fn make_dir_recursive(path: &str) -> GodotResult<()> {
        std::fs::create_dir_all(translate(path)).map_err(|e| GodotError::Io(e.to_string()))
    }
    pub fn list_dir(path: &str) -> GodotResult<Vec<String>> {
        let real = translate(path);
        let mut out = vec![];
        for entry in std::fs::read_dir(&real).map_err(|e| GodotError::Io(e.to_string()))? {
            let entry = entry.map_err(|e| GodotError::Io(e.to_string()))?;
            if let Some(name) = entry.file_name().to_str() {
                if name == "." || name == ".." {
                    continue;
                }
                out.push(name.to_owned());
            }
        }
        out.sort();
        Ok(out)
    }
    pub fn remove(path: &str) -> GodotResult<()> {
        let real = translate(path);
        if real.is_dir() {
            std::fs::remove_dir_all(&real).map_err(|e| GodotError::Io(e.to_string()))
        } else {
            std::fs::remove_file(&real).map_err(|e| GodotError::Io(e.to_string()))
        }
    }
}

pub fn ensure_user_dir() -> PathBuf {
    let d = user_dir();
    let _ = std::fs::create_dir_all(&d);
    d
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn file_roundtrip_user() {
        let _ = ensure_user_dir();
        let path = "user://_test_host_file.txt";
        let mut w = FileAccess::open_write(path).unwrap();
        w.write_text("line1\nline2\n");
        w.close().unwrap();
        assert!(FileAccess::exists(path));
        let mut r = FileAccess::open_read(path).unwrap();
        assert_eq!(r.get_line().as_deref(), Some("line1"));
        assert_eq!(r.get_line().as_deref(), Some("line2"));
        assert_eq!(r.get_line(), None);
        let _ = DirAccess::remove(path);
    }
    #[test]
    fn dir_list_and_prefixes() {
        assert!(translate("res://x").ends_with(std::path::Path::new("x")));
        assert!(translate("user://y").to_string_lossy().contains("godot_rs_user"));
        let _ = ensure_user_dir();
        let files = DirAccess::list_dir("user://").unwrap();
        assert!(files.iter().all(|f| f != "." && f != ".."));
    }
}
