use crate::state::CanvasState;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct SyncInfo {
    pub room_id: String,
    pub server_url: String,
    pub public_share_token: String,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct MetadataInfo {
    pub title: String,
    pub created_at: String,
    pub last_synced_at: String,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct KugelCloudPointer {
    pub version: String,
    pub kugelsh_schema: String,
    pub sync: SyncInfo,
    pub metadata: MetadataInfo,
    pub offline_snapshot: CanvasState,
}

impl KugelCloudPointer {
    pub fn new(room_id: String, server_url: String, share_token: String, title: String, snapshot: CanvasState) -> Self {
        let now = chrono::Utc::now().to_rfc3339();
        Self {
            version: "1.0".to_string(),
            kugelsh_schema: "1.0".to_string(),
            sync: SyncInfo {
                room_id,
                server_url,
                public_share_token: share_token,
            },
            metadata: MetadataInfo {
                title,
                created_at: now.clone(),
                last_synced_at: now,
            },
            offline_snapshot: snapshot,
        }
    }

    pub fn is_kugelsh_path(path: &Path) -> bool {
        path.extension()
            .and_then(|ext| ext.to_str())
            .map(|ext| ext.eq_ignore_ascii_case("kugelsh"))
            .unwrap_or(false)
    }

    /// Read .kugelsh file safely
    pub fn read_from_file(path: &Path) -> Result<Self, String> {
        let content = fs::read_to_string(path).map_err(|e| format!("Failed to read file: {e}"))?;
        serde_json::from_str(&content).map_err(|e| format!("Invalid .kugelsh JSON schema: {e}"))
    }

    /// Save .kugelsh atomically to disk using .tmp file + atomic rename
    pub fn save_atomic(&self, path: &Path) -> Result<(), String> {
        let json = serde_json::to_string_pretty(self)
            .map_err(|e| format!("Serialization error: {e}"))?;

        let tmp_path = path.with_extension("kugelsh.tmp");
        {
            let mut f = fs::File::create(&tmp_path)
                .map_err(|e| format!("Failed to create tmp file: {e}"))?;
            f.write_all(json.as_bytes())
                .map_err(|e| format!("Failed to write tmp file: {e}"))?;
            f.flush().map_err(|e| format!("Flush error: {e}"))?;
        }

        fs::rename(&tmp_path, path).map_err(|e| format!("Atomic rename failed: {e}"))
    }
}

/// Internal Cache Manager for real-time live edits (~/.local/share/kugel/cache/<room_id>.json)
pub struct LocalRoomCache;

impl LocalRoomCache {
    fn cache_dir() -> PathBuf {
        let base = dirs_next::data_local_dir()
            .unwrap_or_else(|| PathBuf::from("~/.local/share"));
        base.join("kugel").join("cache")
    }

    pub fn cache_path_for_room(room_id: &str) -> PathBuf {
        Self::cache_dir().join(format!("{room_id}.json"))
    }

    pub fn save_cache(room_id: &str, state: &CanvasState) -> Result<(), String> {
        let path = Self::cache_path_for_room(room_id);
        if let Some(parent) = path.parent() {
            let _ = fs::create_dir_all(parent);
        }
        let json = serde_json::to_string(state).map_err(|e| format!("Cache JSON error: {e}"))?;
        fs::write(path, json).map_err(|e| format!("Cache write error: {e}"))
    }

    pub fn load_cache(room_id: &str) -> Option<CanvasState> {
        let path = Self::cache_path_for_room(room_id);
        if path.exists() {
            if let Ok(content) = fs::read_to_string(path) {
                if let Ok(state) = serde_json::from_str(&content) {
                    return Some(state);
                }
            }
        }
        None
    }
}

/// Content-Addressable Storage (CAS) for compressed base64 / binary images
pub struct CasAsset;

impl CasAsset {
    pub fn compute_sha256(bytes: &[u8]) -> String {
        let mut hasher = Sha256::new();
        hasher.update(bytes);
        format!("{:x}", hasher.finalize())
    }

    /// Internal CAS cache path: ~/.local/share/kugel/assets/<sha256>.bin
    pub fn asset_cache_path(hash: &str) -> PathBuf {
        let base = dirs_next::data_local_dir()
            .unwrap_or_else(|| PathBuf::from("~/.local/share"));
        base.join("kugel").join("assets").join(hash)
    }

    pub fn save_local(hash: &str, bytes: &[u8]) -> Result<PathBuf, String> {
        let path = Self::asset_cache_path(hash);
        if let Some(parent) = path.parent() {
            let _ = fs::create_dir_all(parent);
        }
        if !path.exists() {
            fs::write(&path, bytes).map_err(|e| format!("CAS write error: {e}"))?;
        }
        Ok(path)
    }

    pub fn load_local(hash: &str) -> Option<Vec<u8>> {
        let path = Self::asset_cache_path(hash);
        if path.exists() {
            return fs::read(path).ok();
        }
        None
    }
}

mod dirs_next {
    use std::path::PathBuf;
    pub fn data_local_dir() -> Option<PathBuf> {
        #[cfg(target_os = "macos")]
        {
            if let Some(home) = std::env::var_os("HOME") {
                return Some(PathBuf::from(home).join("Library").join("Application Support"));
            }
        }
        #[cfg(not(target_os = "macos"))]
        {
            if let Some(home) = std::env::var_os("HOME") {
                return Some(PathBuf::from(home).join(".local").join("share"));
            }
        }
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_kugelsh_atomic_save_and_read() {
        let temp_dir = tempfile::tempdir().expect("tempdir");
        let file_path = temp_dir.path().join("test_board.kugelsh");

        let canvas_state = CanvasState {
            version: "1.0".to_string(),
            shapes: vec![],
            background_color: [20, 20, 23, 255],
            zoom: 1.25,
            pan_offset: [10.0, -20.0],
            next_id: 1,
            dark_mode: true,
        };

        let pointer = KugelCloudPointer::new(
            "room_uuid_123".to_string(),
            "wss://api.kugel.app/v1/sync".to_string(),
            "token_abc".to_string(),
            "Test Project".to_string(),
            canvas_state,
        );

        pointer.save_atomic(&file_path).expect("Atomic save failed");
        assert!(file_path.exists());

        let loaded = KugelCloudPointer::read_from_file(&file_path).expect("Read failed");
        assert_eq!(loaded.sync.room_id, "room_uuid_123");
        assert_eq!(loaded.metadata.title, "Test Project");
        assert_eq!(loaded.offline_snapshot.zoom, 1.25);
    }

    #[test]
    fn test_cas_asset_sha256() {
        let data = b"Sample Image Data 12345";
        let hash = CasAsset::compute_sha256(data);
        assert!(!hash.is_empty());
        assert_eq!(hash.len(), 64); // hex sha256 length
    }
}
