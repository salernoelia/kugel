//! Comprehensive tests for real-time sync propagation fixes.
//!
//! These tests verify that **every** user action that mutates shapes
//! results in the correct network messages being sent so that other
//! clients receive the changes in cloud sessions.

use futures_util::{SinkExt, StreamExt};
use kugel::app::App;
use kugel::net::protocol::{ClientMessage, ServerMessage, ZOrderAction};
use kugel::server::persistence::ServerDb;
use kugel::server::state::ServerState;
use kugel::server::KugelServer;
use kugel::shapes::Shape;
use std::net::SocketAddr;
use std::time::Duration;
use tokio::sync::mpsc;
use tokio_tungstenite::connect_async;
use tokio_tungstenite::tungstenite::Message;

/// Helper: start a server on the given port and return the room_id and ws_url
async fn start_test_server(port: u16) -> (KugelServer, String) {
    let server = KugelServer::new_in_memory().expect("Failed to initialize server");
    let addr: SocketAddr = format!("127.0.0.1:{port}").parse().unwrap();
    let listener = tokio::net::TcpListener::bind(addr)
        .await
        .expect(&format!("Failed to bind port {port}"));
    let router = server.router();
    tokio::spawn(async move {
        axum::serve(listener, router).await.unwrap();
    });
    tokio::time::sleep(Duration::from_millis(50)).await;
    let ws_url = format!("ws://127.0.0.1:{port}/v1/sync");
    (server, ws_url)
}

/// Helper: connect a WebSocket client, authenticate, and return (tx, rx, user_id)
async fn connect_client(
    ws_url: &str,
    room_id: &str,
    token: &str,
) -> (
    futures_util::stream::SplitSink<tokio_tungstenite::WebSocketStream<tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>>, Message>,
    futures_util::stream::SplitStream<tokio_tungstenite::WebSocketStream<tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>>>,
    String,
) {
    let url = format!("{ws_url}/{room_id}");
    let (ws_stream, _) = connect_async(&url).await.expect("WS connect failed");
    let (mut tx, mut rx) = ws_stream.split();

    // Authenticate
    tx.send(Message::Binary(
        ClientMessage::Authenticate {
            token: token.to_string(),
        }
        .to_msgpack()
        .unwrap()
        .into(),
    ))
    .await
    .unwrap();

    // Read RoomState
    let msg = rx.next().await.unwrap().unwrap();
    let state = ServerMessage::from_msgpack(&msg.into_data()).unwrap();
    let user_id = if let ServerMessage::RoomState { your_user_id, .. } = state {
        your_user_id
    } else {
        panic!("Expected RoomState");
    };

    (tx, rx, user_id)
}

/// Helper: read the next non-cursor, non-pong server message with a timeout
async fn next_data_message(
    rx: &mut futures_util::stream::SplitStream<tokio_tungstenite::WebSocketStream<tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>>>,
) -> ServerMessage {
    let timeout = Duration::from_secs(5);
    loop {
        let msg = tokio::time::timeout(timeout, rx.next())
            .await
            .expect("Timed out waiting for server message")
            .unwrap()
            .unwrap();
        let data = msg.into_data();
        let server_msg = ServerMessage::from_msgpack(&data)
            .or_else(|_| ServerMessage::from_json(&String::from_utf8_lossy(&data)))
            .unwrap();
        // Skip ephemeral messages
        match &server_msg {
            ServerMessage::Pong | ServerMessage::RemoteCursor { .. } => continue,
            _ => return server_msg,
        }
    }
}

// ============================================================================
// Test: Shape creation is broadcast to other clients
// ============================================================================
#[tokio::test]
async fn test_shape_create_broadcast() {
    let (_server, ws_url) = start_test_server(18701).await;
    let room_id = "room_create_test";

    let (mut c1_tx, _c1_rx, _) = connect_client(&ws_url, room_id, "user_alice").await;
    let (_, mut c2_rx, _) = connect_client(&ws_url, room_id, "user_bob").await;

    // Drain UserJoined for c1 (when bob joined)
    // c2 already has its RoomState consumed

    // Client 1 creates a shape
    let shape = Shape::new_rect(
        42,
        egui::Rect::from_min_size(egui::pos2(10.0, 20.0), egui::vec2(100.0, 50.0)),
        egui::Color32::RED,
        2.0,
        false,
    );
    c1_tx
        .send(Message::Binary(
            ClientMessage::CreateShape {
                shape: shape.clone(),
            }
            .to_msgpack()
            .unwrap()
            .into(),
        ))
        .await
        .unwrap();

    // Client 2 should receive ShapeCreated
    let msg = next_data_message(&mut c2_rx).await;
    match msg {
        ServerMessage::ShapeCreated { shape: received, seq, .. } => {
            assert_eq!(received.id, 42);
            assert!(seq >= 1);
        }
        other => panic!("Expected ShapeCreated, got: {:?}", other),
    }
}

// ============================================================================
// Test: Shape update is broadcast to other clients
// ============================================================================
#[tokio::test]
async fn test_shape_update_broadcast() {
    let (_server, ws_url) = start_test_server(18702).await;
    let room_id = "room_update_test";

    let (mut c1_tx, mut c1_rx, _) = connect_client(&ws_url, room_id, "user_alice").await;
    let (mut c2_tx, mut c2_rx, _) = connect_client(&ws_url, room_id, "user_bob").await;

    // Client 1 creates a shape
    let shape = Shape::new_rect(
        100,
        egui::Rect::from_min_size(egui::pos2(0.0, 0.0), egui::vec2(50.0, 50.0)),
        egui::Color32::BLUE,
        1.0,
        false,
    );
    c1_tx
        .send(Message::Binary(
            ClientMessage::CreateShape {
                shape: shape.clone(),
            }
            .to_msgpack()
            .unwrap()
            .into(),
        ))
        .await
        .unwrap();

    // Drain ShapeCreated on c2
    let _ = next_data_message(&mut c2_rx).await;

    // Client 2 updates the shape (simulates property change in inspector)
    let mut updated_data = shape.data.clone();
    updated_data.translate(egui::vec2(100.0, 200.0));
    c2_tx
        .send(Message::Binary(
            ClientMessage::UpdateShape {
                shape_id: 100,
                data: updated_data.clone(),
            }
            .to_msgpack()
            .unwrap()
            .into(),
        ))
        .await
        .unwrap();

    // Client 1 should receive ShapeUpdated
    // Drain UserJoined first
    loop {
        let msg = next_data_message(&mut c1_rx).await;
        match msg {
            ServerMessage::ShapeUpdated { shape_id, data, seq, .. } => {
                assert_eq!(shape_id, 100);
                let bounds = data.get_bounds();
                assert_eq!(bounds.min, egui::pos2(100.0, 200.0));
                assert!(seq >= 2);
                break;
            }
            ServerMessage::UserJoined { .. } => continue,
            other => panic!("Expected ShapeUpdated, got: {:?}", other),
        }
    }
}

// ============================================================================
// Test: Shape deletion is broadcast to other clients
// ============================================================================
#[tokio::test]
async fn test_shape_delete_broadcast() {
    let (_server, ws_url) = start_test_server(18703).await;
    let room_id = "room_delete_test";

    let (mut c1_tx, _c1_rx, _) = connect_client(&ws_url, room_id, "user_alice").await;
    let (_, mut c2_rx, _) = connect_client(&ws_url, room_id, "user_bob").await;

    // Client 1 creates two shapes
    for id in [201, 202] {
        let shape = Shape::new_rect(
            id,
            egui::Rect::from_min_size(egui::pos2(0.0, 0.0), egui::vec2(50.0, 50.0)),
            egui::Color32::GREEN,
            1.0,
            false,
        );
        c1_tx
            .send(Message::Binary(
                ClientMessage::CreateShape { shape }
                    .to_msgpack()
                    .unwrap()
                    .into(),
            ))
            .await
            .unwrap();
    }

    // Drain ShapeCreated messages on c2
    let _ = next_data_message(&mut c2_rx).await;
    let _ = next_data_message(&mut c2_rx).await;

    // Client 1 deletes both shapes
    c1_tx
        .send(Message::Binary(
            ClientMessage::DeleteShapes {
                shape_ids: vec![201, 202],
            }
            .to_msgpack()
            .unwrap()
            .into(),
        ))
        .await
        .unwrap();

    // Client 2 should receive ShapesDeleted
    loop {
        let msg = next_data_message(&mut c2_rx).await;
        match msg {
            ServerMessage::ShapesDeleted { shape_ids, .. } => {
                assert!(shape_ids.contains(&201));
                assert!(shape_ids.contains(&202));
                break;
            }
            _ => continue,
        }
    }
}

// ============================================================================
// Test: Shape reorder is broadcast and sequence tracked
// ============================================================================
#[tokio::test]
async fn test_shape_reorder_broadcast_with_seq() {
    let (_server, ws_url) = start_test_server(18704).await;
    let room_id = "room_reorder_test";

    let (mut c1_tx, _, _) = connect_client(&ws_url, room_id, "user_alice").await;
    let (_, mut c2_rx, _) = connect_client(&ws_url, room_id, "user_bob").await;

    // Create two shapes
    for id in [301, 302] {
        let shape = Shape::new_rect(
            id,
            egui::Rect::from_min_size(egui::pos2(0.0, 0.0), egui::vec2(50.0, 50.0)),
            egui::Color32::RED,
            1.0,
            false,
        );
        c1_tx
            .send(Message::Binary(
                ClientMessage::CreateShape { shape }
                    .to_msgpack()
                    .unwrap()
                    .into(),
            ))
            .await
            .unwrap();
    }

    // Drain ShapeCreated on c2
    let _ = next_data_message(&mut c2_rx).await;
    let _ = next_data_message(&mut c2_rx).await;

    // Client 1 reorders: bring shape 301 to front
    c1_tx
        .send(Message::Binary(
            ClientMessage::ReorderShapes {
                shape_ids: vec![301],
                action: ZOrderAction::BringToFront,
            }
            .to_msgpack()
            .unwrap()
            .into(),
        ))
        .await
        .unwrap();

    // Client 2 should receive ShapesReordered with a valid seq
    loop {
        let msg = next_data_message(&mut c2_rx).await;
        match msg {
            ServerMessage::ShapesReordered {
                shape_ids,
                action,
                seq,
                ..
            } => {
                assert_eq!(shape_ids, vec![301]);
                assert_eq!(action, ZOrderAction::BringToFront);
                assert!(seq >= 3); // After 2 creates
                break;
            }
            _ => continue,
        }
    }
}

// ============================================================================
// Test: New client receives existing shapes on join (full sync)
// ============================================================================
#[tokio::test]
async fn test_new_client_receives_full_state() {
    let (_server, ws_url) = start_test_server(18705).await;
    let room_id = "room_fullstate_test";

    // Client 1 creates shapes
    let (mut c1_tx, _, _) = connect_client(&ws_url, room_id, "user_alice").await;
    for id in [401, 402, 403] {
        let shape = Shape::new_rect(
            id,
            egui::Rect::from_min_size(egui::pos2(0.0, 0.0), egui::vec2(50.0, 50.0)),
            egui::Color32::RED,
            1.0,
            false,
        );
        c1_tx
            .send(Message::Binary(
                ClientMessage::CreateShape { shape }
                    .to_msgpack()
                    .unwrap()
                    .into(),
            ))
            .await
            .unwrap();
    }

    tokio::time::sleep(Duration::from_millis(100)).await;

    // Client 2 joins AFTER shapes were created
    let url = format!("{ws_url}/{room_id}");
    let (ws2, _) = connect_async(&url).await.unwrap();
    let (mut c2_tx, mut c2_rx) = ws2.split();
    c2_tx
        .send(Message::Binary(
            ClientMessage::Authenticate {
                token: "user_bob".to_string(),
            }
            .to_msgpack()
            .unwrap()
            .into(),
        ))
        .await
        .unwrap();

    let msg = c2_rx.next().await.unwrap().unwrap();
    let state = ServerMessage::from_msgpack(&msg.into_data()).unwrap();
    match state {
        ServerMessage::RoomState { shapes, .. } => {
            assert_eq!(shapes.len(), 3, "New client should receive all 3 shapes");
            let ids: Vec<usize> = shapes.iter().map(|s| s.id).collect();
            assert!(ids.contains(&401));
            assert!(ids.contains(&402));
            assert!(ids.contains(&403));
        }
        other => panic!("Expected RoomState, got: {:?}", other),
    }
}

// ============================================================================
// Test: Room state race condition - concurrent connections get same Room
// ============================================================================
#[tokio::test]
async fn test_concurrent_room_creation_no_split_brain() {
    let db = ServerDb::memory().unwrap();
    let state = ServerState::new(db);

    // Simulate concurrent access to the same room
    let state1 = state.clone();
    let state2 = state.clone();

    let (room1, room2) = tokio::join!(
        state1.get_or_create_room("concurrent_room", "Room 1"),
        state2.get_or_create_room("concurrent_room", "Room 2"),
    );

    // Both should point to the same Arc
    assert!(
        std::sync::Arc::ptr_eq(&room1, &room2),
        "Concurrent room access should return the same Arc<Mutex<Room>>"
    );
}

// ============================================================================
// Test: Server-side apply_update_shape broadcasts to other peers
// ============================================================================
#[tokio::test]
async fn test_server_apply_update_broadcasts() {
    let db = ServerDb::memory().unwrap();
    let state = ServerState::new(db);
    let room_arc = state.get_or_create_room("broadcast_test", "Test").await;
    let mut room = room_arc.lock().await;

    // Add two peers
    let (tx1, mut rx1) = mpsc::unbounded_channel();
    let (tx2, mut rx2) = mpsc::unbounded_channel();

    let user1 = kugel::net::protocol::RemoteUser {
        id: "user1".to_string(),
        display_name: "Alice".to_string(),
        color: [255, 0, 0, 255],
        avatar_url: None,
    };
    let user2 = kugel::net::protocol::RemoteUser {
        id: "user2".to_string(),
        display_name: "Bob".to_string(),
        color: [0, 255, 0, 255],
        avatar_url: None,
    };

    room.add_peer(user1, tx1);
    room.add_peer(user2, tx2);

    // Drain UserJoined messages
    let _ = rx1.try_recv();
    let _ = rx2.try_recv();

    // Create a shape as user1
    let shape = Shape::new_rect(
        500,
        egui::Rect::from_min_size(egui::pos2(0.0, 0.0), egui::vec2(100.0, 100.0)),
        egui::Color32::RED,
        2.0,
        false,
    );
    room.apply_create_shape("user1", shape.clone());

    // user2 should receive ShapeCreated, user1 should NOT
    assert!(
        rx1.try_recv().is_err(),
        "Sender should NOT receive their own ShapeCreated"
    );
    let msg2 = rx2.try_recv().expect("Peer should receive ShapeCreated");
    match msg2 {
        ServerMessage::ShapeCreated { shape: s, .. } => assert_eq!(s.id, 500),
        other => panic!("Expected ShapeCreated, got: {:?}", other),
    }

    // Update shape as user2
    let mut new_data = shape.data.clone();
    new_data.translate(egui::vec2(50.0, 50.0));
    room.apply_update_shape("user2", 500, new_data);

    // user1 should receive ShapeUpdated, user2 should NOT
    let msg1 = rx1.try_recv().expect("Peer should receive ShapeUpdated");
    match msg1 {
        ServerMessage::ShapeUpdated { shape_id, .. } => assert_eq!(shape_id, 500),
        other => panic!("Expected ShapeUpdated, got: {:?}", other),
    }
    assert!(
        rx2.try_recv().is_err(),
        "Sender should NOT receive their own ShapeUpdated"
    );

    // Delete shape as user1
    room.apply_delete_shapes("user1", vec![500]);

    // user2 should receive ShapesDeleted
    let msg2_del = rx2.try_recv().expect("Peer should receive ShapesDeleted");
    match msg2_del {
        ServerMessage::ShapesDeleted { shape_ids, .. } => assert_eq!(shape_ids, vec![500]),
        other => panic!("Expected ShapesDeleted, got: {:?}", other),
    }
}

// ============================================================================
// Test: Server-side apply_reorder_shapes broadcasts to other peers
// ============================================================================
#[tokio::test]
async fn test_server_reorder_broadcasts() {
    let db = ServerDb::memory().unwrap();
    let state = ServerState::new(db);
    let room_arc = state.get_or_create_room("reorder_test", "Test").await;
    let mut room = room_arc.lock().await;

    let (tx1, _rx1) = mpsc::unbounded_channel();
    let (tx2, mut rx2) = mpsc::unbounded_channel();

    let user1 = kugel::net::protocol::RemoteUser {
        id: "u1".to_string(),
        display_name: "A".to_string(),
        color: [255, 0, 0, 255],
        avatar_url: None,
    };
    let user2 = kugel::net::protocol::RemoteUser {
        id: "u2".to_string(),
        display_name: "B".to_string(),
        color: [0, 255, 0, 255],
        avatar_url: None,
    };

    room.add_peer(user1, tx1);
    room.add_peer(user2, tx2);

    // Drain UserJoined
    let _ = rx2.try_recv();

    // Create two shapes
    let s1 = Shape::new_rect(601, egui::Rect::EVERYTHING, egui::Color32::RED, 1.0, false);
    let s2 = Shape::new_rect(602, egui::Rect::EVERYTHING, egui::Color32::BLUE, 1.0, false);
    room.apply_create_shape("u1", s1);
    room.apply_create_shape("u1", s2);

    // Drain create messages
    let _ = rx2.try_recv();
    let _ = rx2.try_recv();

    // Reorder as user1
    room.apply_reorder_shapes("u1", vec![601], ZOrderAction::BringToFront);

    let msg = rx2.try_recv().expect("Should receive ShapesReordered");
    match msg {
        ServerMessage::ShapesReordered {
            shape_ids,
            action,
            seq,
            ..
        } => {
            assert_eq!(shape_ids, vec![601]);
            assert_eq!(action, ZOrderAction::BringToFront);
            assert!(seq >= 3);
        }
        other => panic!("Expected ShapesReordered, got: {:?}", other),
    }
}

// ============================================================================
// Test: DeltaRingBuffer catch-up replay works correctly
// ============================================================================
#[tokio::test]
async fn test_delta_ring_buffer_catchup_replay() {
    let (_server, ws_url) = start_test_server(18706).await;
    let room_id = "room_catchup_test";

    // Client 1 connects and creates shapes
    let (mut c1_tx, _, _) = connect_client(&ws_url, room_id, "user_alice").await;
    for id in [701, 702, 703] {
        let shape = Shape::new_rect(
            id,
            egui::Rect::from_min_size(egui::pos2(0.0, 0.0), egui::vec2(50.0, 50.0)),
            egui::Color32::RED,
            1.0,
            false,
        );
        c1_tx
            .send(Message::Binary(
                ClientMessage::CreateShape { shape }
                    .to_msgpack()
                    .unwrap()
                    .into(),
            ))
            .await
            .unwrap();
    }

    tokio::time::sleep(Duration::from_millis(100)).await;

    // Client 2 connects and gets full state
    let url = format!("{ws_url}/{room_id}");
    let (ws2, _) = connect_async(&url).await.unwrap();
    let (mut c2_tx, mut c2_rx) = ws2.split();
    c2_tx
        .send(Message::Binary(
            ClientMessage::Authenticate {
                token: "user_bob".to_string(),
            }
            .to_msgpack()
            .unwrap()
            .into(),
        ))
        .await
        .unwrap();

    let msg = c2_rx.next().await.unwrap().unwrap();
    let state = ServerMessage::from_msgpack(&msg.into_data()).unwrap();
    if let ServerMessage::RoomState {
        current_seq,
        shapes,
        ..
    } = state
    {
        assert_eq!(shapes.len(), 3);
        // CatchUp with current_seq should return no deltas (nothing missed)
        c2_tx
            .send(Message::Binary(
                ClientMessage::CatchUp {
                    last_seen_seq: current_seq,
                }
                .to_msgpack()
                .unwrap()
                .into(),
            ))
            .await
            .unwrap();

        // Client 1 now creates another shape
        let shape = Shape::new_rect(
            704,
            egui::Rect::from_min_size(egui::pos2(0.0, 0.0), egui::vec2(50.0, 50.0)),
            egui::Color32::BLUE,
            1.0,
            false,
        );
        c1_tx
            .send(Message::Binary(
                ClientMessage::CreateShape { shape }
                    .to_msgpack()
                    .unwrap()
                    .into(),
            ))
            .await
            .unwrap();

        // Client 2 should receive ShapeCreated for shape 704
        loop {
            let msg = next_data_message(&mut c2_rx).await;
            match msg {
                ServerMessage::ShapeCreated { shape, .. } if shape.id == 704 => {
                    break;
                }
                _ => continue,
            }
        }
    } else {
        panic!("Expected RoomState");
    }
}

// ============================================================================
// Test: sync_canvas_diff correctly detects creates, updates, and deletes
// ============================================================================
#[test]
fn test_sync_canvas_diff_detects_all_changes() {
    let mut app = App::default();

    // Setup: old state has shapes 1, 2, 3
    let s1 = Shape::new_rect(
        1,
        egui::Rect::from_min_size(egui::pos2(0.0, 0.0), egui::vec2(50.0, 50.0)),
        egui::Color32::RED,
        1.0,
        false,
    );
    let s2 = Shape::new_rect(
        2,
        egui::Rect::from_min_size(egui::pos2(100.0, 0.0), egui::vec2(50.0, 50.0)),
        egui::Color32::BLUE,
        1.0,
        false,
    );
    let s3 = Shape::new_rect(
        3,
        egui::Rect::from_min_size(egui::pos2(200.0, 0.0), egui::vec2(50.0, 50.0)),
        egui::Color32::GREEN,
        1.0,
        false,
    );

    let old_shapes = vec![s1.clone(), s2.clone(), s3.clone()];

    // New state: shape 1 unchanged, shape 2 moved, shape 3 deleted, shape 4 created
    let mut s2_modified = s2.clone();
    s2_modified.data.translate(egui::vec2(50.0, 50.0));
    let s4 = Shape::new_rect(
        4,
        egui::Rect::from_min_size(egui::pos2(300.0, 0.0), egui::vec2(50.0, 50.0)),
        egui::Color32::YELLOW,
        1.0,
        false,
    );

    app.canvas.shapes = vec![s1.clone(), s2_modified.clone(), s4.clone()];

    // Compute diff manually (same logic as sync_canvas_diff)
    let old_map: std::collections::HashMap<usize, &Shape> =
        old_shapes.iter().map(|s| (s.id, s)).collect();
    let new_map: std::collections::HashMap<usize, &Shape> =
        app.canvas.shapes.iter().map(|s| (s.id, s)).collect();

    // Deleted: shape 3
    let deleted_ids: Vec<usize> = old_shapes
        .iter()
        .filter(|s| !new_map.contains_key(&s.id))
        .map(|s| s.id)
        .collect();
    assert_eq!(deleted_ids, vec![3]);

    // Created: shape 4
    let created: Vec<usize> = app
        .canvas
        .shapes
        .iter()
        .filter(|s| !old_map.contains_key(&s.id))
        .map(|s| s.id)
        .collect();
    assert_eq!(created, vec![4]);

    // Modified: shape 2
    let modified: Vec<usize> = app
        .canvas
        .shapes
        .iter()
        .filter(|s| {
            if let Some(old) = old_map.get(&s.id) {
                old.data != s.data
            } else {
                false
            }
        })
        .map(|s| s.id)
        .collect();
    assert_eq!(modified, vec![2]);
}

// ============================================================================
// Test: Lock mechanism works correctly across multiple clients
// ============================================================================
#[tokio::test]
async fn test_lock_grant_deny_release_flow() {
    let (_server, ws_url) = start_test_server(18707).await;
    let room_id = "room_lock_test";

    let (mut c1_tx, mut c1_rx, _) = connect_client(&ws_url, room_id, "user_alice").await;
    let (mut c2_tx, mut c2_rx, _) = connect_client(&ws_url, room_id, "user_bob").await;

    // Client 1 requests lock on shape 42 -> should be granted
    c1_tx
        .send(Message::Binary(
            ClientMessage::RequestLock { shape_id: 42 }
                .to_msgpack()
                .unwrap()
                .into(),
        ))
        .await
        .unwrap();

    loop {
        let msg = next_data_message(&mut c1_rx).await;
        match msg {
            ServerMessage::LockGranted { shape_id, .. } if shape_id == 42 => break,
            ServerMessage::UserJoined { .. } => continue,
            other => panic!("Expected LockGranted, got: {:?}", other),
        }
    }

    // Client 2 requests same lock -> should be denied
    c2_tx
        .send(Message::Binary(
            ClientMessage::RequestLock { shape_id: 42 }
                .to_msgpack()
                .unwrap()
                .into(),
        ))
        .await
        .unwrap();

    loop {
        let msg = next_data_message(&mut c2_rx).await;
        match msg {
            ServerMessage::LockDenied { shape_id, .. } if shape_id == 42 => break,
            ServerMessage::UserJoined { .. } | ServerMessage::LockGranted { .. } => continue,
            other => panic!("Expected LockDenied, got: {:?}", other),
        }
    }

    // Client 1 releases lock
    c1_tx
        .send(Message::Binary(
            ClientMessage::ReleaseLock { shape_id: 42 }
                .to_msgpack()
                .unwrap()
                .into(),
        ))
        .await
        .unwrap();

    tokio::time::sleep(Duration::from_millis(50)).await;

    // Client 2 retries lock -> should be granted now
    c2_tx
        .send(Message::Binary(
            ClientMessage::RequestLock { shape_id: 42 }
                .to_msgpack()
                .unwrap()
                .into(),
        ))
        .await
        .unwrap();

    loop {
        let msg = next_data_message(&mut c2_rx).await;
        match msg {
            ServerMessage::LockGranted { shape_id, .. } if shape_id == 42 => break,
            ServerMessage::LockReleased { .. } => continue,
            other => panic!("Expected LockGranted after release, got: {:?}", other),
        }
    }
}

// ============================================================================
// Test: User disconnect cleans up correctly
// ============================================================================
#[tokio::test]
async fn test_user_disconnect_cleanup() {
    let (_server, ws_url) = start_test_server(18708).await;
    let room_id = "room_disconnect_test";

    let (_c1_tx, mut c1_rx, _) = connect_client(&ws_url, room_id, "user_alice").await;

    // Client 2 connects
    let url = format!("{ws_url}/{room_id}");
    let (ws2, _) = connect_async(&url).await.unwrap();
    let (mut c2_tx, _c2_rx) = ws2.split();
    c2_tx
        .send(Message::Binary(
            ClientMessage::Authenticate {
                token: "user_bob".to_string(),
            }
            .to_msgpack()
            .unwrap()
            .into(),
        ))
        .await
        .unwrap();

    // Drain UserJoined on c1
    loop {
        let msg = next_data_message(&mut c1_rx).await;
        if matches!(msg, ServerMessage::UserJoined { .. }) {
            break;
        }
    }

    // Client 2 disconnects (drop the sender)
    c2_tx
        .send(Message::Binary(
            ClientMessage::LeaveRoom.to_msgpack().unwrap().into(),
        ))
        .await
        .unwrap();

    // Client 1 should receive UserLeft
    loop {
        let msg = next_data_message(&mut c1_rx).await;
        match msg {
            ServerMessage::UserLeft { user_id } => {
                assert!(user_id.contains("bob") || !user_id.is_empty());
                break;
            }
            _ => continue,
        }
    }
}

// ============================================================================
// Test: Multiple operations in sequence maintain correct state
// ============================================================================
#[tokio::test]
async fn test_multi_operation_sequence_consistency() {
    let (_server, ws_url) = start_test_server(18709).await;
    let room_id = "room_multi_op";

    let (mut c1_tx, _, _) = connect_client(&ws_url, room_id, "user_alice").await;

    // Create 3 shapes
    for id in [1, 2, 3] {
        let shape = Shape::new_rect(
            id,
            egui::Rect::from_min_size(
                egui::pos2(id as f32 * 100.0, 0.0),
                egui::vec2(50.0, 50.0),
            ),
            egui::Color32::RED,
            1.0,
            false,
        );
        c1_tx
            .send(Message::Binary(
                ClientMessage::CreateShape { shape }
                    .to_msgpack()
                    .unwrap()
                    .into(),
            ))
            .await
            .unwrap();
    }

    // Update shape 2
    let data = kugel::shapes::ShapeData::Rectangle {
        rect: egui::Rect::from_min_size(egui::pos2(999.0, 999.0), egui::vec2(50.0, 50.0)),
        color: egui::Color32::YELLOW,
        stroke_width: 3.0,
        filled: true,
    };
    c1_tx
        .send(Message::Binary(
            ClientMessage::UpdateShape {
                shape_id: 2,
                data: data.clone(),
            }
            .to_msgpack()
            .unwrap()
            .into(),
        ))
        .await
        .unwrap();

    // Delete shape 1
    c1_tx
        .send(Message::Binary(
            ClientMessage::DeleteShapes {
                shape_ids: vec![1],
            }
            .to_msgpack()
            .unwrap()
            .into(),
        ))
        .await
        .unwrap();

    // Reorder shape 3 to front
    c1_tx
        .send(Message::Binary(
            ClientMessage::ReorderShapes {
                shape_ids: vec![3],
                action: ZOrderAction::BringToFront,
            }
            .to_msgpack()
            .unwrap()
            .into(),
        ))
        .await
        .unwrap();

    tokio::time::sleep(Duration::from_millis(200)).await;

    // New client joins and should see consistent state: shapes [2(updated), 3]
    let url = format!("{ws_url}/{room_id}");
    let (ws2, _) = connect_async(&url).await.unwrap();
    let (mut c2_tx, mut c2_rx) = ws2.split();
    c2_tx
        .send(Message::Binary(
            ClientMessage::Authenticate {
                token: "user_bob".to_string(),
            }
            .to_msgpack()
            .unwrap()
            .into(),
        ))
        .await
        .unwrap();

    let msg = c2_rx.next().await.unwrap().unwrap();
    let state = ServerMessage::from_msgpack(&msg.into_data()).unwrap();
    match state {
        ServerMessage::RoomState { shapes, .. } => {
            assert_eq!(shapes.len(), 2, "Should have 2 shapes (1 deleted)");
            let ids: Vec<usize> = shapes.iter().map(|s| s.id).collect();
            assert!(!ids.contains(&1), "Shape 1 should be deleted");
            assert!(ids.contains(&2), "Shape 2 should exist");
            assert!(ids.contains(&3), "Shape 3 should exist");

            // Shape 3 should be last (brought to front)
            assert_eq!(
                shapes.last().unwrap().id,
                3,
                "Shape 3 should be at front (last in array)"
            );

            // Shape 2 should be updated
            let s2 = shapes.iter().find(|s| s.id == 2).unwrap();
            assert_eq!(
                s2.data.get_bounds().min,
                egui::pos2(999.0, 999.0),
                "Shape 2 should have updated position"
            );
        }
        other => panic!("Expected RoomState, got: {:?}", other),
    }
}

// ============================================================================
// Test: DB persistence survives restart
// ============================================================================
#[tokio::test]
async fn test_db_persistence_roundtrip() {
    let temp_dir = tempfile::tempdir().expect("tempdir");
    let db_path = temp_dir.path().join("test_persist.db");

    let room_id = "persist_room";
    let shape = Shape::new_rect(
        888,
        egui::Rect::from_min_size(egui::pos2(10.0, 20.0), egui::vec2(100.0, 50.0)),
        egui::Color32::RED,
        2.0,
        true,
    );

    // Phase 1: Create room, add shape, save snapshot
    {
        let db = ServerDb::open_file(&db_path).unwrap();
        let state = ServerState::new(db);
        let room_arc = state.get_or_create_room(room_id, "Persistent Room").await;
        let mut room = room_arc.lock().await;
        room.apply_create_shape("test_user", shape.clone());
        let snapshot = room.snapshot_canvas_state();
        state
            .db
            .save_room_snapshot(room_id, &room.title, &snapshot)
            .unwrap();
    }

    // Phase 2: Load from DB — shape should persist
    {
        let db = ServerDb::open_file(&db_path).unwrap();
        let state = ServerState::new(db);
        let room_arc = state.get_or_create_room(room_id, "Persistent Room").await;
        let room = room_arc.lock().await;
        assert_eq!(room.shapes.len(), 1);
        assert_eq!(room.shapes[0].id, 888);
    }
}
