use crate::state::CanvasState;
use rusqlite::{params, Connection};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

#[derive(Clone)]
pub struct ServerDb {
    conn: Arc<Mutex<Connection>>,
}

impl ServerDb {
    pub fn memory() -> Result<Self, String> {
        let conn = Connection::open_in_memory().map_err(|e| e.to_string())?;
        let db = Self {
            conn: Arc::new(Mutex::new(conn)),
        };
        db.init_tables()?;
        Ok(db)
    }

    pub fn open_file(path: &PathBuf) -> Result<Self, String> {
        if let Some(parent) = path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        let conn = Connection::open(path).map_err(|e| e.to_string())?;
        let db = Self {
            conn: Arc::new(Mutex::new(conn)),
        };
        db.init_tables()?;
        Ok(db)
    }

    fn init_tables(&self) -> Result<(), String> {
        let conn = self.conn.lock().unwrap();
        conn.execute_batch(
            "
            CREATE TABLE IF NOT EXISTS rooms (
                room_id TEXT PRIMARY KEY,
                title TEXT NOT NULL,
                created_at TEXT NOT NULL,
                snapshot_json TEXT NOT NULL
            );

            CREATE TABLE IF NOT EXISTS assets (
                hash TEXT PRIMARY KEY,
                data BLOB NOT NULL,
                created_at TEXT NOT NULL
            );

            CREATE TABLE IF NOT EXISTS users (
                user_id TEXT PRIMARY KEY,
                email TEXT UNIQUE,
                display_name TEXT NOT NULL,
                created_at TEXT NOT NULL
            );
            ",
        )
        .map_err(|e| format!("Init DB schema error: {e}"))?;
        Ok(())
    }

    pub fn save_room_snapshot(&self, room_id: &str, title: &str, state: &CanvasState) -> Result<(), String> {
        let conn = self.conn.lock().unwrap();
        let snapshot_json = serde_json::to_string(state).map_err(|e| e.to_string())?;
        let now = chrono::Utc::now().to_rfc3339();

        conn.execute(
            "INSERT INTO rooms (room_id, title, created_at, snapshot_json)
             VALUES (?1, ?2, ?3, ?4)
             ON CONFLICT(room_id) DO UPDATE SET title=?2, snapshot_json=?4",
            params![room_id, title, now, snapshot_json],
        )
        .map_err(|e| e.to_string())?;
        Ok(())
    }

    pub fn load_room_snapshot(&self, room_id: &str) -> Result<Option<CanvasState>, String> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn
            .prepare("SELECT snapshot_json FROM rooms WHERE room_id = ?1")
            .map_err(|e| e.to_string())?;

        let mut rows = stmt.query(params![room_id]).map_err(|e| e.to_string())?;
        if let Some(row) = rows.next().map_err(|e| e.to_string())? {
            let json: String = row.get(0).map_err(|e| e.to_string())?;
            let state: CanvasState = serde_json::from_str(&json).map_err(|e| e.to_string())?;
            Ok(Some(state))
        } else {
            Ok(None)
        }
    }

    pub fn save_asset(&self, hash: &str, data: &[u8]) -> Result<(), String> {
        let conn = self.conn.lock().unwrap();
        let now = chrono::Utc::now().to_rfc3339();
        conn.execute(
            "INSERT OR IGNORE INTO assets (hash, data, created_at) VALUES (?1, ?2, ?3)",
            params![hash, data, now],
        )
        .map_err(|e| e.to_string())?;
        Ok(())
    }

    pub fn load_asset(&self, hash: &str) -> Result<Option<Vec<u8>>, String> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn
            .prepare("SELECT data FROM assets WHERE hash = ?1")
            .map_err(|e| e.to_string())?;
        let mut rows = stmt.query(params![hash]).map_err(|e| e.to_string())?;

        if let Some(row) = rows.next().map_err(|e| e.to_string())? {
            let blob: Vec<u8> = row.get(0).map_err(|e| e.to_string())?;
            Ok(Some(blob))
        } else {
            Ok(None)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_server_db_snapshot_and_asset() {
        let db = ServerDb::memory().expect("Memory DB creation failed");

        let state = CanvasState {
            version: "1.0".to_string(),
            shapes: vec![],
            background_color: [0, 0, 0, 255],
            zoom: 1.0,
            pan_offset: [0.0, 0.0],
            next_id: 1,
            dark_mode: true,
        };

        db.save_room_snapshot("room_1", "My Room", &state).unwrap();
        let loaded = db.load_room_snapshot("room_1").unwrap().expect("Room missing");
        assert_eq!(loaded.version, "1.0");

        db.save_asset("sha256_hash_1", b"binary_image_data").unwrap();
        let asset = db.load_asset("sha256_hash_1").unwrap().expect("Asset missing");
        assert_eq!(asset, b"binary_image_data");
    }
}
