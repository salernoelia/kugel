use crate::shapes::{Shape, ShapeData};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
pub enum ZOrderAction {
    BringToFront,
    SendToBack,
    BringForward,
    SendBackward,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct RemoteUser {
    pub id: String,
    pub display_name: String,
    pub color: [u8; 4],
    pub avatar_url: Option<String>,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub enum ClientMessage {
    // Auth & Room Management
    Authenticate { token: String },
    JoinRoom { room_code: String },
    LeaveRoom,

    // Reconnection Catch-Up
    CatchUp { last_seen_seq: u64 },

    // High-frequency Ephemeral Presence (~30-60Hz)
    CursorMove { x: f32, y: f32, selected_ids: Vec<usize> },

    // Shape Mutation & Locking
    RequestLock { shape_id: usize },
    ReleaseLock { shape_id: usize },
    UpdateShape { shape_id: usize, data: ShapeData },
    CreateShape { shape: Shape },
    DeleteShapes { shape_ids: Vec<usize> },
    ReorderShapes { shape_ids: Vec<usize>, action: ZOrderAction },

    // Heartbeat
    Ping,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub enum ServerMessage {
    // Room Session Setup
    RoomState {
        room_code: String,
        users: Vec<RemoteUser>,
        shapes: Vec<Shape>,
        locked_shapes: HashMap<usize, String>, // ShapeId -> UserId
        current_seq: u64,
        your_user_id: String,
    },
    UserJoined { user: RemoteUser },
    UserLeft { user_id: String },

    // Real-Time Events
    RemoteCursor {
        user_id: String,
        x: f32,
        y: f32,
        selected_ids: Vec<usize>,
    },
    LockGranted { shape_id: usize, user_id: String },
    LockDenied { shape_id: usize, owner_id: String },
    LockReleased { shape_id: usize },
    ShapeUpdated {
        user_id: String,
        shape_id: usize,
        data: ShapeData,
        seq: u64,
    },
    ShapeCreated {
        user_id: String,
        shape: Shape,
        seq: u64,
    },
    ShapesDeleted {
        user_id: String,
        shape_ids: Vec<usize>,
        seq: u64,
    },
    ShapesReordered {
        user_id: String,
        shape_ids: Vec<usize>,
        action: ZOrderAction,
        seq: u64,
    },

    // System
    Pong,
    Error { message: String },
}

impl ClientMessage {
    pub fn to_msgpack(&self) -> Result<Vec<u8>, rmp_serde::encode::Error> {
        rmp_serde::to_vec_named(self)
    }

    pub fn from_msgpack(bytes: &[u8]) -> Result<Self, rmp_serde::decode::Error> {
        rmp_serde::from_slice(bytes)
    }

    pub fn to_json(&self) -> Result<String, serde_json::Error> {
        serde_json::to_string(self)
    }

    pub fn from_json(json: &str) -> Result<Self, serde_json::Error> {
        serde_json::from_str(json)
    }
}

impl ServerMessage {
    pub fn to_msgpack(&self) -> Result<Vec<u8>, rmp_serde::encode::Error> {
        rmp_serde::to_vec_named(self)
    }

    pub fn from_msgpack(bytes: &[u8]) -> Result<Self, rmp_serde::decode::Error> {
        rmp_serde::from_slice(bytes)
    }

    pub fn to_json(&self) -> Result<String, serde_json::Error> {
        serde_json::to_string(self)
    }

    pub fn from_json(json: &str) -> Result<Self, serde_json::Error> {
        serde_json::from_str(json)
    }
}

pub fn default_server_url() -> String {
    std::env::var("KUGEL_SERVER_URL").unwrap_or_else(|_| "ws://127.0.0.1:8765".to_string())
}

pub fn parse_room_link(input: &str) -> (String, String) {
    let clean = input.trim();
    if let Some(rest) = clean.strip_prefix("kugel://room/") {
        if let Some((room_id, query)) = rest.split_once('?') {
            let mut server_url = default_server_url();
            for param in query.split('&') {
                if let Some((key, val)) = param.split_once('=') {
                    if key.eq_ignore_ascii_case("server") && !val.is_empty() {
                        server_url = val.to_string();
                    }
                }
            }
            return (room_id.to_string(), server_url);
        } else {
            return (rest.to_string(), default_server_url());
        }
    }
    (clean.to_string(), default_server_url())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_client_message_msgpack_roundtrip() {
        let msg = ClientMessage::CursorMove {
            x: 150.5,
            y: 300.25,
            selected_ids: vec![1, 2, 3],
        };
        let encoded = msg.to_msgpack().expect("Encoding failed");
        let decoded = ClientMessage::from_msgpack(&encoded).expect("Decoding failed");
        if let ClientMessage::CursorMove { x, y, selected_ids } = decoded {
            assert_eq!(x, 150.5);
            assert_eq!(y, 300.25);
            assert_eq!(selected_ids, vec![1, 2, 3]);
        } else {
            panic!("Variant mismatch");
        }
    }

    #[test]
    fn test_server_message_msgpack_roundtrip() {
        let user = RemoteUser {
            id: "user-123".to_string(),
            display_name: "Alice".to_string(),
            color: [255, 0, 0, 255],
            avatar_url: None,
        };
        let msg = ServerMessage::UserJoined { user: user.clone() };
        let encoded = msg.to_msgpack().expect("Encoding failed");
        let decoded = ServerMessage::from_msgpack(&encoded).expect("Decoding failed");
        if let ServerMessage::UserJoined { user: decoded_user } = decoded {
            assert_eq!(decoded_user, user);
        } else {
            panic!("Variant mismatch");
        }
    }
}
