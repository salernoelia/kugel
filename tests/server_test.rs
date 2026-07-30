use futures_util::{SinkExt, StreamExt};
use kugel::net::protocol::{ClientMessage, ServerMessage};
use kugel::server::KugelServer;
use std::net::SocketAddr;
use std::time::Duration;
use tokio_tungstenite::connect_async;
use tokio_tungstenite::tungstenite::Message;

#[tokio::test]
async fn test_full_server_rest_ws_and_locking() {
    let server = KugelServer::new_in_memory().expect("Failed to initialize server");
    let addr: SocketAddr = "127.0.0.1:8765".parse().unwrap();

    // Spawn server in background
    let listener = tokio::net::TcpListener::bind(addr).await.expect("Failed to bind port 8765");
    tokio::spawn(async move {
        axum::serve(listener, server.router()).await.unwrap();
    });

    tokio::time::sleep(Duration::from_millis(100)).await;

    // 1. REST API Room Creation Test
    let http_client = reqwest::Client::new();
    let res = http_client
        .post("http://127.0.0.1:8765/api/v1/rooms")
        .json(&serde_json::json!({ "title": "Integration Test Room" }))
        .send()
        .await
        .expect("Failed HTTP room creation");

    assert_eq!(res.status(), 200);
    let room_res: serde_json::Value = res.json().await.unwrap();
    let room_id = room_res["room_id"].as_str().unwrap().to_string();
    assert!(!room_id.is_empty());

    // 2. CAS Asset Upload & Fetch Test
    let asset_bytes = b"Sample PNG Image Binary Payload 123";
    let upload_res = http_client
        .post("http://127.0.0.1:8765/api/v1/assets")
        .body(asset_bytes.to_vec())
        .send()
        .await
        .expect("Asset upload failed");
    assert_eq!(upload_res.status(), 200);
    let asset_json: serde_json::Value = upload_res.json().await.unwrap();
    let hash = asset_json["hash"].as_str().unwrap();

    let fetch_res = http_client
        .get(&format!("http://127.0.0.1:8765/api/v1/assets/{hash}"))
        .send()
        .await
        .expect("Asset fetch failed");
    assert_eq!(fetch_res.status(), 200);
    assert_eq!(fetch_res.bytes().await.unwrap().as_ref(), asset_bytes);

    // 3. Multi-Client WebSocket & Lock Mechanics Test
    let ws_url = format!("ws://127.0.0.1:8765/v1/sync/{room_id}");

    // Client 1 Connection
    let (ws_stream1, _) = connect_async(&ws_url).await.expect("Client 1 WS connect failed");
    let (mut client1_tx, mut client1_rx) = ws_stream1.split();

    let auth_c1 = ClientMessage::Authenticate { token: "user_client1".to_string() };
    client1_tx.send(Message::Binary(auth_c1.to_msgpack().unwrap().into())).await.unwrap();

    // Read RoomState response for Client 1
    let state_msg1 = client1_rx.next().await.unwrap().unwrap();
    let server_msg1 = ServerMessage::from_msgpack(&state_msg1.into_data()).unwrap();
    if let ServerMessage::RoomState { room_code, .. } = server_msg1 {
        assert_eq!(room_code, room_id);
    } else {
        panic!("Expected RoomState for Client 1");
    }

    // Client 2 Connection
    let (ws_stream2, _) = connect_async(&ws_url).await.expect("Client 2 WS connect failed");
    let (mut client2_tx, mut client2_rx) = ws_stream2.split();

    let auth_c2 = ClientMessage::Authenticate { token: "user_client2".to_string() };
    client2_tx.send(Message::Binary(auth_c2.to_msgpack().unwrap().into())).await.unwrap();

    let state_msg2 = client2_rx.next().await.unwrap().unwrap();
    assert!(matches!(ServerMessage::from_msgpack(&state_msg2.into_data()).unwrap(), ServerMessage::RoomState { .. }));

    // Client 1 requests lock on shape 42 -> Granted
    let req_lock = ClientMessage::RequestLock { shape_id: 42 };
    client1_tx.send(Message::Binary(req_lock.to_msgpack().unwrap().into())).await.unwrap();

    let mut lock1_granted = false;
    while let Some(Ok(msg)) = client1_rx.next().await {
        if let Ok(ServerMessage::LockGranted { shape_id, user_id }) = ServerMessage::from_msgpack(&msg.into_data()) {
            if shape_id == 42 && user_id.contains("client1") {
                lock1_granted = true;
                break;
            }
        }
    }
    assert!(lock1_granted, "Client 1 should receive LockGranted");

    // Client 2 requests lock on same shape 42 -> Denied!
    let req_lock_c2 = ClientMessage::RequestLock { shape_id: 42 };
    client2_tx.send(Message::Binary(req_lock_c2.to_msgpack().unwrap().into())).await.unwrap();

    let mut lock2_denied = false;
    while let Some(Ok(msg)) = client2_rx.next().await {
        if let Ok(ServerMessage::LockDenied { shape_id, owner_id }) = ServerMessage::from_msgpack(&msg.into_data()) {
            if shape_id == 42 && owner_id.contains("client1") {
                lock2_denied = true;
                break;
            }
        }
    }
    assert!(lock2_denied, "Client 2 should receive LockDenied");

    // Client 1 releases lock
    let rel_lock = ClientMessage::ReleaseLock { shape_id: 42 };
    client1_tx.send(Message::Binary(rel_lock.to_msgpack().unwrap().into())).await.unwrap();

    // Client 2 now requests lock -> Granted!
    client2_tx.send(Message::Binary(req_lock_c2.to_msgpack().unwrap().into())).await.unwrap();

    // Drain until LockGranted for client 2
    let mut granted = false;
    while let Some(Ok(m)) = client2_rx.next().await {
        if let Ok(ServerMessage::LockGranted { shape_id, user_id }) = ServerMessage::from_msgpack(&m.into_data()) {
            if shape_id == 42 && user_id == "user_client2" {
                granted = true;
                break;
            }
        }
    }
    assert!(granted, "Client 2 should be granted lock after release");
}
