use crate::net::protocol::{ClientMessage, RemoteUser, ServerMessage};
use crate::server::locks::LockResult;
use crate::server::state::ServerState;
use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use axum::extract::{Path, State};
use axum::response::IntoResponse;
use futures_util::{SinkExt, StreamExt};
use std::time::{Duration, Instant};
use tokio::sync::mpsc;

pub async fn ws_handler(
    ws: WebSocketUpgrade,
    Path(room_id): Path<String>,
    State(state): State<ServerState>,
) -> impl IntoResponse {
    ws.on_upgrade(move |socket| handle_socket(socket, room_id, state))
}

async fn handle_socket(socket: WebSocket, room_id: String, state: ServerState) {
    let (mut ws_tx, mut ws_rx) = socket.split();

    let (peer_tx, mut peer_rx) = mpsc::unbounded_channel::<ServerMessage>();

    // Spawn task to forward outgoing ServerMessages to WebSocket
    tokio::spawn(async move {
        while let Some(msg) = peer_rx.recv().await {
            let bytes = match msg.to_msgpack() {
                Ok(b) => b,
                Err(_) => continue,
            };
            if ws_tx.send(Message::Binary(bytes.into())).await.is_err() {
                break;
            }
        }
    });

    let room_arc = state.get_or_create_room(&room_id, "Collaborative Board").await;
    let mut current_user: Option<RemoteUser> = None;

    // Rate Limiting (Token Bucket: 300 msg/sec)
    let mut last_rate_check = Instant::now();
    let mut msg_count = 0u32;

    while let Some(Ok(msg)) = ws_rx.next().await {
        // Enforce rate limiting
        msg_count += 1;
        if last_rate_check.elapsed() >= Duration::from_secs(1) {
            last_rate_check = Instant::now();
            msg_count = 0;
        } else if msg_count > 300 {
            let _ = peer_tx.send(ServerMessage::Error {
                message: "Rate limit exceeded (300 msg/sec max)".to_string(),
            });
            continue;
        }

        let bytes = match msg {
            Message::Binary(b) => b.to_vec(),
            Message::Text(t) => t.into_bytes(),
            Message::Ping(_p) => {
                let _ = peer_tx.send(ServerMessage::Pong);
                continue;
            }
            Message::Close(_) => break,
            _ => continue,
        };

        if bytes.len() > 5 * 1024 * 1024 {
            let _ = peer_tx.send(ServerMessage::Error {
                message: "Frame size exceeds 5MB limit".to_string(),
            });
            continue;
        }

        // Try decoding MessagePack, fallback to JSON
        let client_msg = ClientMessage::from_msgpack(&bytes)
            .or_else(|_| ClientMessage::from_json(&String::from_utf8_lossy(&bytes)));

        let client_msg = match client_msg {
            Ok(m) => m,
            Err(e) => {
                let _ = peer_tx.send(ServerMessage::Error {
                    message: format!("Deserialization error: {e}"),
                });
                continue;
            }
        };

        match client_msg {
            ClientMessage::Authenticate { token } => {
                // Generate guest user or authenticate
                let user_id = if token.starts_with("user_") {
                    token.clone()
                } else {
                    format!("guest_{:08x}", rand::random::<u32>())
                };

                let user = RemoteUser {
                    id: user_id.clone(),
                    display_name: format!("Collaborator {}", &user_id[..user_id.len().min(6)]),
                    color: [
                        rand::random::<u8>().max(80),
                        rand::random::<u8>().max(80),
                        rand::random::<u8>().max(80),
                        255,
                    ],
                    avatar_url: None,
                };

                current_user = Some(user.clone());

                let mut room = room_arc.lock().await;
                room.add_peer(user.clone(), peer_tx.clone());

                let initial_state = ServerMessage::RoomState {
                    room_code: room.room_id.clone(),
                    users: room.get_users(),
                    shapes: room.shapes.clone(),
                    locked_shapes: room.lock_manager.get_locks(),
                    current_seq: room.seq_no.load(std::sync::atomic::Ordering::SeqCst),
                    your_user_id: user.id.clone(),
                };
                let _ = peer_tx.send(initial_state);
            }

            ClientMessage::JoinRoom { room_code: _ } => {
                let mut room = room_arc.lock().await;
                if let Some(ref user) = current_user {
                    room.add_peer(user.clone(), peer_tx.clone());
                    let _ = peer_tx.send(ServerMessage::RoomState {
                        room_code: room.room_id.clone(),
                        users: room.get_users(),
                        shapes: room.shapes.clone(),
                        locked_shapes: room.lock_manager.get_locks(),
                        current_seq: room.seq_no.load(std::sync::atomic::Ordering::SeqCst),
                        your_user_id: user.id.clone(),
                    });
                }
            }

            ClientMessage::CatchUp { last_seen_seq } => {
                let room = room_arc.lock().await;
                if let Some(deltas) = room.delta_ring_buffer.get_deltas_since(last_seen_seq) {
                    for (_seq, delta_msg) in deltas {
                        let _ = peer_tx.send(delta_msg);
                    }
                } else {
                    // Evicted from ring buffer -> send full snapshot
                    let user_id = current_user.as_ref().map(|u| u.id.clone()).unwrap_or_default();
                    let _ = peer_tx.send(ServerMessage::RoomState {
                        room_code: room.room_id.clone(),
                        users: room.get_users(),
                        shapes: room.shapes.clone(),
                        locked_shapes: room.lock_manager.get_locks(),
                        current_seq: room.seq_no.load(std::sync::atomic::Ordering::SeqCst),
                        your_user_id: user_id,
                    });
                }
            }

            ClientMessage::CursorMove { x, y, selected_ids } => {
                if let Some(ref user) = current_user {
                    let room = room_arc.lock().await;
                    let cursor_msg = ServerMessage::RemoteCursor {
                        user_id: user.id.clone(),
                        x,
                        y,
                        selected_ids,
                    };
                    room.broadcast(&cursor_msg, Some(&user.id));
                }
            }

            ClientMessage::RequestLock { shape_id } => {
                if let Some(ref user) = current_user {
                    let mut room = room_arc.lock().await;
                    match room.lock_manager.request_lock(shape_id, &user.id) {
                        LockResult::Granted => {
                            let msg = ServerMessage::LockGranted {
                                shape_id,
                                user_id: user.id.clone(),
                            };
                            let _ = peer_tx.send(msg.clone());
                            room.broadcast(&msg, Some(&user.id));
                        }
                        LockResult::Denied { owner_id } => {
                            let _ = peer_tx.send(ServerMessage::LockDenied { shape_id, owner_id });
                        }
                    }
                }
            }

            ClientMessage::ReleaseLock { shape_id } => {
                if let Some(ref user) = current_user {
                    let mut room = room_arc.lock().await;
                    if room.lock_manager.release_lock(shape_id, &user.id) {
                        let msg = ServerMessage::LockReleased { shape_id };
                        let _ = peer_tx.send(msg.clone());
                        room.broadcast(&msg, Some(&user.id));
                    }
                }
            }

            ClientMessage::UpdateShape { shape_id, data } => {
                if let Some(ref user) = current_user {
                    let mut room = room_arc.lock().await;
                    room.apply_update_shape(&user.id, shape_id, data);
                    let snapshot = room.snapshot_canvas_state();
                    let _ = state.db.save_room_snapshot(&room.room_id, &room.title, &snapshot);
                }
            }

            ClientMessage::CreateShape { shape } => {
                if let Some(ref user) = current_user {
                    let mut room = room_arc.lock().await;
                    room.apply_create_shape(&user.id, shape);
                    let snapshot = room.snapshot_canvas_state();
                    let _ = state.db.save_room_snapshot(&room.room_id, &room.title, &snapshot);
                }
            }

            ClientMessage::DeleteShapes { shape_ids } => {
                if let Some(ref user) = current_user {
                    let mut room = room_arc.lock().await;
                    room.apply_delete_shapes(&user.id, shape_ids);
                    let snapshot = room.snapshot_canvas_state();
                    let _ = state.db.save_room_snapshot(&room.room_id, &room.title, &snapshot);
                }
            }

            ClientMessage::ReorderShapes { shape_ids, action } => {
                if let Some(ref user) = current_user {
                    let mut room = room_arc.lock().await;
                    room.apply_reorder_shapes(&user.id, shape_ids, action);
                    let snapshot = room.snapshot_canvas_state();
                    let _ = state.db.save_room_snapshot(&room.room_id, &room.title, &snapshot);
                }
            }

            ClientMessage::Ping => {
                let _ = peer_tx.send(ServerMessage::Pong);
            }

            ClientMessage::LeaveRoom => {
                break;
            }
        }
    }

    // Clean up peer connection on disconnect
    if let Some(user) = current_user {
        let mut room = room_arc.lock().await;
        room.remove_peer(&user.id);
    }
}
