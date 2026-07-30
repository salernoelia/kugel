use crate::net::crdt::{DeltaRingBuffer, FractionalZIndex};
use crate::net::protocol::{RemoteUser, ServerMessage};
use crate::server::locks::LockManager;
use crate::server::persistence::ServerDb;
use crate::shapes::{Shape, ShapeData};
use crate::state::CanvasState;
use dashmap::DashMap;
use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use tokio::sync::{mpsc, Mutex};

pub struct PeerConnection {
    pub user: RemoteUser,
    pub tx: mpsc::UnboundedSender<ServerMessage>,
}

pub struct Room {
    pub room_id: String,
    pub title: String,
    pub shapes: Vec<Shape>,
    pub z_indices: HashMap<usize, String>,
    pub lock_manager: LockManager,
    pub seq_no: AtomicU64,
    pub delta_ring_buffer: DeltaRingBuffer,
    pub peers: HashMap<String, PeerConnection>,
}

impl Room {
    pub fn new(room_id: String, title: String, initial_shapes: Vec<Shape>) -> Self {
        let mut z_indices = HashMap::new();
        let mut prev_key: Option<String> = None;

        for shape in &initial_shapes {
            let key = FractionalZIndex::generate_between(prev_key.as_deref(), None);
            z_indices.insert(shape.id, key.clone());
            prev_key = Some(key);
        }

        Self {
            room_id,
            title,
            shapes: initial_shapes,
            z_indices,
            lock_manager: LockManager::default(),
            seq_no: AtomicU64::new(1),
            delta_ring_buffer: DeltaRingBuffer::new(1000),
            peers: HashMap::new(),
        }
    }

    pub fn next_seq(&self) -> u64 {
        self.seq_no.fetch_add(1, Ordering::SeqCst)
    }

    pub fn broadcast(&self, msg: &ServerMessage, exclude_user_id: Option<&str>) {
        for (user_id, peer) in &self.peers {
            if let Some(exclude) = exclude_user_id {
                if user_id == exclude {
                    continue;
                }
            }
            let _ = peer.tx.send(msg.clone());
        }
    }

    pub fn add_peer(&mut self, user: RemoteUser, tx: mpsc::UnboundedSender<ServerMessage>) {
        let user_id = user.id.clone();

        // Broadcast UserJoined to all existing peers
        let joined_msg = ServerMessage::UserJoined { user: user.clone() };
        self.broadcast(&joined_msg, Some(&user_id));

        self.peers.insert(user_id, PeerConnection { user, tx });
    }

    pub fn remove_peer(&mut self, user_id: &str) {
        if self.peers.remove(user_id).is_some() {
            // Auto release any locks held by leaving user
            let released_locks = self.lock_manager.release_all_for_user(user_id);
            for shape_id in released_locks {
                self.broadcast(&ServerMessage::LockReleased { shape_id }, None);
            }

            self.broadcast(
                &ServerMessage::UserLeft {
                    user_id: user_id.to_string(),
                },
                None,
            );
        }
    }

    pub fn get_users(&self) -> Vec<RemoteUser> {
        self.peers.values().map(|p| p.user.clone()).collect()
    }

    pub fn apply_update_shape(
        &mut self,
        user_id: &str,
        shape_id: usize,
        data: ShapeData,
    ) -> Option<u64> {
        if let Some(shape) = self.shapes.iter_mut().find(|s| s.id == shape_id) {
            shape.data = data.clone();
            let seq = self.next_seq();
            let msg = ServerMessage::ShapeUpdated {
                user_id: user_id.to_string(),
                shape_id,
                data,
                seq,
            };
            self.delta_ring_buffer.push(seq, msg.clone());
            self.broadcast(&msg, Some(user_id));
            Some(seq)
        } else {
            None
        }
    }

    pub fn apply_create_shape(&mut self, user_id: &str, mut shape: Shape) -> u64 {
        let seq = self.next_seq();
        if shape.id == 0 {
            shape.id = (seq as usize) + 1000;
        }

        if let Some(existing) = self.shapes.iter_mut().find(|s| s.id == shape.id) {
            existing.data = shape.data.clone();
            let msg = ServerMessage::ShapeUpdated {
                user_id: user_id.to_string(),
                shape_id: shape.id,
                data: shape.data,
                seq,
            };
            self.delta_ring_buffer.push(seq, msg.clone());
            self.broadcast(&msg, Some(user_id));
            return seq;
        }

        // Generate Z-index key
        let last_key = self.shapes.last().and_then(|s| self.z_indices.get(&s.id)).cloned();
        let new_key = FractionalZIndex::generate_between(last_key.as_deref(), None);
        self.z_indices.insert(shape.id, new_key);

        self.shapes.push(shape.clone());

        let msg = ServerMessage::ShapeCreated {
            user_id: user_id.to_string(),
            shape: shape,
            seq,
        };
        self.delta_ring_buffer.push(seq, msg.clone());
        self.broadcast(&msg, Some(user_id));
        seq
    }

    pub fn apply_delete_shapes(&mut self, user_id: &str, shape_ids: Vec<usize>) -> u64 {
        let seq = self.next_seq();
        for id in &shape_ids {
            self.shapes.retain(|s| s.id != *id);
            self.z_indices.remove(id);
            self.lock_manager.release_lock(*id, user_id);
        }

        let msg = ServerMessage::ShapesDeleted {
            user_id: user_id.to_string(),
            shape_ids,
            seq,
        };
        self.delta_ring_buffer.push(seq, msg.clone());
        self.broadcast(&msg, Some(user_id));
        seq
    }

    pub fn apply_reorder_shapes(
        &mut self,
        user_id: &str,
        shape_ids: Vec<usize>,
        action: crate::net::protocol::ZOrderAction,
    ) -> u64 {
        use crate::net::protocol::ZOrderAction;
        let seq = self.next_seq();

        match action {
            ZOrderAction::BringToFront => {
                let mut moved = Vec::new();
                self.shapes.retain(|s| {
                    if shape_ids.contains(&s.id) {
                        moved.push(s.clone());
                        false
                    } else {
                        true
                    }
                });
                self.shapes.extend(moved);
            }
            ZOrderAction::SendToBack => {
                let mut moved = Vec::new();
                self.shapes.retain(|s| {
                    if shape_ids.contains(&s.id) {
                        moved.push(s.clone());
                        false
                    } else {
                        true
                    }
                });
                moved.extend(self.shapes.clone());
                self.shapes = moved;
            }
            ZOrderAction::BringForward => {
                for i in (0..self.shapes.len().saturating_sub(1)).rev() {
                    if shape_ids.contains(&self.shapes[i].id) {
                        self.shapes.swap(i, i + 1);
                    }
                }
            }
            ZOrderAction::SendBackward => {
                for i in 1..self.shapes.len() {
                    if shape_ids.contains(&self.shapes[i].id) {
                        self.shapes.swap(i, i - 1);
                    }
                }
            }
        }

        let msg = ServerMessage::ShapesReordered {
            user_id: user_id.to_string(),
            shape_ids,
            action,
            seq,
        };
        self.delta_ring_buffer.push(seq, msg.clone());
        self.broadcast(&msg, Some(user_id));
        seq
    }

    pub fn snapshot_canvas_state(&self) -> CanvasState {
        CanvasState {
            version: "1.0".to_string(),
            shapes: self.shapes.clone(),
            background_color: [20, 20, 23, 255],
            zoom: 1.0,
            pan_offset: [0.0, 0.0],
            next_id: self.shapes.iter().map(|s| s.id).max().unwrap_or(0) + 1,
            dark_mode: true,
        }
    }
}

#[derive(Clone)]
pub struct ServerState {
    pub rooms: Arc<DashMap<String, Arc<Mutex<Room>>>>,
    pub db: ServerDb,
}

impl ServerState {
    pub fn new(db: ServerDb) -> Self {
        Self {
            rooms: Arc::new(DashMap::new()),
            db,
        }
    }

    pub async fn get_or_create_room(&self, room_id: &str, default_title: &str) -> Arc<Mutex<Room>> {
        // Use entry API for atomic check-and-insert to prevent race conditions
        // where two concurrent connections create separate Room instances
        let entry = self.rooms.entry(room_id.to_string());
        match entry {
            dashmap::mapref::entry::Entry::Occupied(o) => o.get().clone(),
            dashmap::mapref::entry::Entry::Vacant(v) => {
                let initial_shapes = match self.db.load_room_snapshot(room_id) {
                    Ok(Some(snapshot)) => snapshot.shapes,
                    _ => Vec::new(),
                };
                let room = Arc::new(Mutex::new(Room::new(
                    room_id.to_string(),
                    default_title.to_string(),
                    initial_shapes,
                )));
                v.insert(room.clone());
                room
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_server_room_lifecycle() {
        let db = ServerDb::memory().unwrap();
        let state = ServerState::new(db);

        let room_arc = state.get_or_create_room("room_xyz", "Test Room").await;
        let mut room = room_arc.lock().await;

        let (tx, _rx) = mpsc::unbounded_channel();
        let user = RemoteUser {
            id: "u1".to_string(),
            display_name: "Alice".to_string(),
            color: [255, 0, 0, 255],
            avatar_url: None,
        };

        room.add_peer(user, tx);
        assert_eq!(room.get_users().len(), 1);

        let rect_shape = Shape::new_rect(10, egui::Rect::EVERYTHING, egui::Color32::RED, 2.0, false);
        let seq = room.apply_create_shape("u1", rect_shape);
        assert_eq!(seq, 1);
        assert_eq!(room.shapes.len(), 1);
    }
}
