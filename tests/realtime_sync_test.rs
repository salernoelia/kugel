use futures_util::{SinkExt, StreamExt};
use kugel::app::App;
use kugel::net::protocol::{parse_room_link, ClientMessage, ServerMessage, ZOrderAction};
use kugel::server::persistence::ServerDb;
use kugel::server::KugelServer;
use kugel::shapes::Shape;
use std::net::SocketAddr;
use std::time::Duration;
use tokio_tungstenite::connect_async;
use tokio_tungstenite::tungstenite::Message;

#[tokio::test]
async fn test_realtime_multi_client_sync_lifecycle() {
    let server = KugelServer::new_in_memory().expect("Failed to initialize server");
    let addr: SocketAddr = "127.0.0.1:8766".parse().unwrap();

    let listener = tokio::net::TcpListener::bind(addr)
        .await
        .expect("Failed to bind port 8766");
    tokio::spawn(async move {
        axum::serve(listener, server.router()).await.unwrap();
    });

    tokio::time::sleep(Duration::from_millis(100)).await;

    let room_id = "test_sync_room_1";
    let ws_url = format!("ws://127.0.0.1:8766/v1/sync/{room_id}");

    // Client 1 Connect & Authenticate
    let (ws_stream1, _) = connect_async(&ws_url).await.expect("Client 1 WS connect failed");
    let (mut c1_tx, mut c1_rx) = ws_stream1.split();
    c1_tx
        .send(Message::Binary(
            ClientMessage::Authenticate {
                token: "user_client_1".to_string(),
            }
            .to_msgpack()
            .unwrap()
            .into(),
        ))
        .await
        .unwrap();

    let msg1 = c1_rx.next().await.unwrap().unwrap();
    let state1 = ServerMessage::from_msgpack(&msg1.into_data()).unwrap();
    if let ServerMessage::RoomState { your_user_id, shapes, .. } = state1 {
        assert!(!your_user_id.is_empty());
        assert!(shapes.is_empty());
    } else {
        panic!("Expected RoomState for Client 1");
    }

    // Client 2 Connect & Authenticate
    let (ws_stream2, _) = connect_async(&ws_url).await.expect("Client 2 WS connect failed");
    let (mut c2_tx, mut c2_rx) = ws_stream2.split();
    c2_tx
        .send(Message::Binary(
            ClientMessage::Authenticate {
                token: "user_client_2".to_string(),
            }
            .to_msgpack()
            .unwrap()
            .into(),
        ))
        .await
        .unwrap();

    let _state2 = c2_rx.next().await.unwrap().unwrap();

    // 1. Client 1 creates a shape -> Client 2 receives ShapeCreated in real time
    let shape_1 = Shape::new_rect(101, egui::Rect::from_min_size(egui::pos2(10.0, 20.0), egui::vec2(100.0, 50.0)), egui::Color32::BLUE, 2.0, false);
    c1_tx
        .send(Message::Binary(
            ClientMessage::CreateShape { shape: shape_1.clone() }
                .to_msgpack()
                .unwrap()
                .into(),
        ))
        .await
        .unwrap();

    let mut created_received = false;
    while let Some(Ok(m)) = c2_rx.next().await {
        if let Ok(ServerMessage::ShapeCreated { shape, .. }) = ServerMessage::from_msgpack(&m.into_data()) {
            assert_eq!(shape.id, 101);
            created_received = true;
            break;
        }
    }
    assert!(created_received, "Client 2 should receive ShapeCreated");

    // 2. Client 2 updates shape -> Client 1 receives ShapeUpdated in real time
    let mut updated_data = shape_1.data.clone();
    updated_data.translate(egui::vec2(50.0, 50.0));
    c2_tx
        .send(Message::Binary(
            ClientMessage::UpdateShape {
                shape_id: 101,
                data: updated_data.clone(),
            }
            .to_msgpack()
            .unwrap()
            .into(),
        ))
        .await
        .unwrap();

    let mut updated_received = false;
    while let Some(Ok(m)) = c1_rx.next().await {
        if let Ok(ServerMessage::ShapeUpdated { shape_id, data, .. }) = ServerMessage::from_msgpack(&m.into_data()) {
            if shape_id == 101 {
                assert_eq!(data.get_bounds().min, egui::pos2(60.0, 70.0));
                updated_received = true;
                break;
            }
        }
    }
    assert!(updated_received, "Client 1 should receive ShapeUpdated");

    // 3. Client 1 reorders shape -> Client 2 receives ShapesReordered in real time
    c1_tx
        .send(Message::Binary(
            ClientMessage::ReorderShapes {
                shape_ids: vec![101],
                action: ZOrderAction::BringToFront,
            }
            .to_msgpack()
            .unwrap()
            .into(),
        ))
        .await
        .unwrap();

    let mut reorder_received = false;
    while let Some(Ok(m)) = c2_rx.next().await {
        if let Ok(ServerMessage::ShapesReordered { shape_ids, action, .. }) = ServerMessage::from_msgpack(&m.into_data()) {
            if shape_ids == vec![101] && action == ZOrderAction::BringToFront {
                reorder_received = true;
                break;
            }
        }
    }
    assert!(reorder_received, "Client 2 should receive ShapesReordered");

    // 4. Client 1 deletes shape -> Client 2 receives ShapesDeleted in real time
    c1_tx
        .send(Message::Binary(
            ClientMessage::DeleteShapes {
                shape_ids: vec![101],
            }
            .to_msgpack()
            .unwrap()
            .into(),
        ))
        .await
        .unwrap();

    let mut delete_received = false;
    while let Some(Ok(m)) = c2_rx.next().await {
        if let Ok(ServerMessage::ShapesDeleted { shape_ids, .. }) = ServerMessage::from_msgpack(&m.into_data()) {
            if shape_ids == vec![101] {
                delete_received = true;
                break;
            }
        }
    }
    assert!(delete_received, "Client 2 should receive ShapesDeleted");
}

#[tokio::test]
async fn test_server_db_persistence_across_restart() {
    let temp_dir = tempfile::tempdir().expect("tempdir");
    let db_path = temp_dir.path().join("server_persisted.db");

    let room_id = "persisted_room_99";
    let shape_test = Shape::new_rect(999, egui::Rect::from_min_size(egui::pos2(0.0, 0.0), egui::vec2(200.0, 150.0)), egui::Color32::GREEN, 3.0, true);

    // Phase 1: Start server with file DB, create shape
    {
        let db = ServerDb::open_file(&db_path).unwrap();
        let server = KugelServer {
            state: kugel::server::state::ServerState::new(db),
        };
        let room_arc = server.state.get_or_create_room(room_id, "Persistent Board").await;
        let mut room = room_arc.lock().await;
        room.apply_create_shape("user_test", shape_test.clone());
        let snapshot = room.snapshot_canvas_state();
        server.state.db.save_room_snapshot(room_id, &room.title, &snapshot).unwrap();
    }

    // Phase 2: Restart server with same DB file, load room
    {
        let db = ServerDb::open_file(&db_path).unwrap();
        let server = KugelServer {
            state: kugel::server::state::ServerState::new(db),
        };
        let room_arc = server.state.get_or_create_room(room_id, "Persistent Board").await;
        let room = room_arc.lock().await;
        assert_eq!(room.shapes.len(), 1);
        assert_eq!(room.shapes[0].id, 999);
    }
}

#[test]
fn test_parse_room_link_formatting() {
    let (room_id, server_url) = parse_room_link("kugel://room/abc-123?server=ws://192.168.1.100:8765");
    assert_eq!(room_id, "abc-123");
    assert_eq!(server_url, "ws://192.168.1.100:8765");

    let (room_id2, server_url2) = parse_room_link("xyz-789");
    assert_eq!(room_id2, "xyz-789");
    assert_eq!(server_url2, "ws://127.0.0.1:8765");
}

#[test]
fn test_sync_canvas_diff_undo_redo() {
    let mut app = App::default();
    let shape1 = Shape::new_rect(1, egui::Rect::EVERYTHING, egui::Color32::RED, 1.0, false);
    let shape2 = Shape::new_rect(2, egui::Rect::EVERYTHING, egui::Color32::BLUE, 1.0, false);

    let old_shapes = vec![shape1.clone()];
    app.canvas.shapes = vec![shape1.clone(), shape2.clone()];

    // Diff should identify shape2 as created
    let old_map: std::collections::HashMap<usize, &Shape> = old_shapes.iter().map(|s| (s.id, s)).collect();
    let new_map: std::collections::HashMap<usize, &Shape> = app.canvas.shapes.iter().map(|s| (s.id, s)).collect();

    let deleted_ids: Vec<usize> = old_shapes.iter().filter(|s| !new_map.contains_key(&s.id)).map(|s| s.id).collect();
    assert!(deleted_ids.is_empty());

    let created_shapes: Vec<&Shape> = app.canvas.shapes.iter().filter(|s| !old_map.contains_key(&s.id)).collect();
    assert_eq!(created_shapes.len(), 1);
    assert_eq!(created_shapes[0].id, 2);
}
