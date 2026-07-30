use std::collections::HashMap;
use std::time::{Duration, Instant};

#[derive(Debug)]
struct LockInfo {
    owner_id: String,
    last_heartbeat: Instant,
}

#[derive(Debug)]
pub struct LockManager {
    locks: HashMap<usize, LockInfo>,
    timeout: Duration,
}

impl Default for LockManager {
    fn default() -> Self {
        Self {
            locks: HashMap::new(),
            timeout: Duration::from_millis(3000), // 3000ms heartbeat auto-release
        }
    }
}

pub enum LockResult {
    Granted,
    Denied { owner_id: String },
}

impl LockManager {
    pub fn new(timeout: Duration) -> Self {
        Self {
            locks: HashMap::new(),
            timeout,
        }
    }

    pub fn request_lock(&mut self, shape_id: usize, user_id: &str) -> LockResult {
        let now = Instant::now();
        if let Some(info) = self.locks.get(&shape_id) {
            if info.owner_id != user_id && now.duration_since(info.last_heartbeat) < self.timeout {
                return LockResult::Denied {
                    owner_id: info.owner_id.clone(),
                };
            }
        }

        self.locks.insert(
            shape_id,
            LockInfo {
                owner_id: user_id.to_string(),
                last_heartbeat: now,
            },
        );
        LockResult::Granted
    }

    pub fn release_lock(&mut self, shape_id: usize, user_id: &str) -> bool {
        if let Some(info) = self.locks.get(&shape_id) {
            if info.owner_id == user_id {
                self.locks.remove(&shape_id);
                return true;
            }
        }
        false
    }

    pub fn release_all_for_user(&mut self, user_id: &str) -> Vec<usize> {
        let mut released = Vec::new();
        self.locks.retain(|shape_id, info| {
            if info.owner_id == user_id {
                released.push(*shape_id);
                false
            } else {
                true
            }
        });
        released
    }

    pub fn cleanup_expired(&mut self) -> Vec<usize> {
        let now = Instant::now();
        let timeout = self.timeout;
        let mut expired = Vec::new();

        self.locks.retain(|shape_id, info| {
            if now.duration_since(info.last_heartbeat) >= timeout {
                expired.push(*shape_id);
                false
            } else {
                true
            }
        });
        expired
    }

    pub fn get_locks(&self) -> HashMap<usize, String> {
        let now = Instant::now();
        let timeout = self.timeout;
        self.locks
            .iter()
            .filter(|(_, info)| now.duration_since(info.last_heartbeat) < timeout)
            .map(|(id, info)| (*id, info.owner_id.clone()))
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_lock_grant_deny_and_release() {
        let mut mgr = LockManager::default();

        // User A requests lock
        match mgr.request_lock(42, "user_a") {
            LockResult::Granted => {}
            _ => panic!("Should be granted"),
        }

        // User B requests lock -> denied
        match mgr.request_lock(42, "user_b") {
            LockResult::Denied { owner_id } => assert_eq!(owner_id, "user_a"),
            _ => panic!("Should be denied"),
        }

        // User A releases lock
        assert!(mgr.release_lock(42, "user_a"));

        // User B requests lock again -> granted
        match mgr.request_lock(42, "user_b") {
            LockResult::Granted => {}
            _ => panic!("Should be granted"),
        }
    }

    #[test]
    fn test_lock_heartbeat_timeout() {
        let mut mgr = LockManager::new(Duration::from_millis(50));
        mgr.request_lock(100, "user_a");

        std::thread::sleep(Duration::from_millis(60));

        // Expired -> User B can claim it
        match mgr.request_lock(100, "user_b") {
            LockResult::Granted => {}
            _ => panic!("Should be granted after timeout"),
        }
    }
}
