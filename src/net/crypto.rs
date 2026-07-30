use aes_gcm::aead::{Aead, KeyInit};
use aes_gcm::{Aes256Gcm, Nonce};
use rand::RngCore;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct UserCredentials {
    pub user_id: String,
    pub display_name: String,
    pub auth_token: String,
    pub color: [u8; 4],
}

impl Default for UserCredentials {
    fn default() -> Self {
        let mut rng = rand::thread_rng();
        let random_id = format!("user_{:08x}", rng.next_u32());
        let guest_name = format!("Guest Falcon_{:04x}", rng.next_u32() % 0x10000);
        let color = [
            (rng.next_u32() % 200 + 55) as u8,
            (rng.next_u32() % 200 + 55) as u8,
            (rng.next_u32() % 200 + 55) as u8,
            255,
        ];
        Self {
            user_id: random_id,
            display_name: guest_name,
            auth_token: format!("jwt_guest_{:016x}", rng.next_u64()),
            color,
        }
    }
}

pub struct CredentialsStore;

impl CredentialsStore {
    fn credentials_path() -> PathBuf {
        let base = dirs_next::data_local_dir()
            .unwrap_or_else(|| PathBuf::from("~/.local/share"));
        base.join("kugel").join("credentials.json")
    }

    pub fn load() -> UserCredentials {
        let path = Self::credentials_path();
        if path.exists() {
            if let Ok(data) = fs::read_to_string(&path) {
                if let Ok(creds) = serde_json::from_str::<UserCredentials>(&data) {
                    return creds;
                }
            }
        }
        let creds = UserCredentials::default();
        let _ = Self::save(&creds);
        creds
    }

    pub fn save(creds: &UserCredentials) -> Result<(), std::io::Error> {
        let path = Self::credentials_path();
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        let json = serde_json::to_string_pretty(creds)?;
        fs::write(path, json)?;
        Ok(())
    }
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct EncryptedFrame {
    pub nonce: [u8; 12],
    pub ciphertext: Vec<u8>,
    pub seq_no: u64,
    pub timestamp_ms: u64,
}

pub struct E2eeCipher {
    cipher: Aes256Gcm,
}

impl E2eeCipher {
    pub fn new_from_key(key_32_bytes: &[u8; 32]) -> Self {
        let cipher = Aes256Gcm::new_from_slice(key_32_bytes)
            .expect("Invalid key length for AES-256");
        Self { cipher }
    }

    pub fn derive_room_key(room_secret: &str) -> [u8; 32] {
        use sha2::{Digest, Sha256};
        let mut hasher = Sha256::new();
        hasher.update(b"kugel_e2ee_salt_v1:");
        hasher.update(room_secret.as_bytes());
        let result = hasher.finalize();
        let mut key = [0u8; 32];
        key.copy_from_slice(&result);
        key
    }

    pub fn encrypt(&self, plaintext: &[u8], seq_no: u64) -> Result<EncryptedFrame, String> {
        let mut nonce_bytes = [0u8; 12];
        rand::thread_rng().fill_bytes(&mut nonce_bytes);
        let nonce = Nonce::from_slice(&nonce_bytes);

        let timestamp_ms = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as u64;

        let ciphertext = self
            .cipher
            .encrypt(nonce, plaintext)
            .map_err(|e| format!("Encryption error: {:?}", e))?;

        Ok(EncryptedFrame {
            nonce: nonce_bytes,
            ciphertext,
            seq_no,
            timestamp_ms,
        })
    }

    pub fn decrypt(&self, frame: &EncryptedFrame) -> Result<Vec<u8>, String> {
        let nonce = Nonce::from_slice(&frame.nonce);
        self.cipher
            .decrypt(nonce, frame.ciphertext.as_slice())
            .map_err(|e| format!("Decryption error: {:?}", e))
    }
}

// Fallback module for dirs_next if not using dirs_next crate directly
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
    fn test_e2ee_encrypt_decrypt_roundtrip() {
        let room_key = E2eeCipher::derive_room_key("secret_room_key_123");
        let cipher = E2eeCipher::new_from_key(&room_key);

        let plaintext = b"Hello, Kugel E2EE real-time sync!";
        let frame = cipher.encrypt(plaintext, 101).expect("Encrypt failed");

        assert_eq!(frame.seq_no, 101);
        let decrypted = cipher.decrypt(&frame).expect("Decrypt failed");
        assert_eq!(decrypted, plaintext);
    }

    #[test]
    fn test_credentials_store_load_save() {
        let temp_dir = tempfile::tempdir().expect("tempdir");
        let test_path = temp_dir.path().join("credentials.json");

        let creds = UserCredentials {
            user_id: "u_test_1".to_string(),
            display_name: "Test User".to_string(),
            auth_token: "jwt_secret_token".to_string(),
            color: [100, 150, 200, 255],
        };

        let json = serde_json::to_string_pretty(&creds).unwrap();
        fs::write(&test_path, json).unwrap();

        let loaded: UserCredentials = serde_json::from_str(&fs::read_to_string(&test_path).unwrap()).unwrap();
        assert_eq!(loaded, creds);
    }
}
