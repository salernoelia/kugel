use kugel::net::protocol::{ClientMessage, RemoteUser, ServerMessage, ZOrderAction};
use kugel::shapes::Shape;
use eframe::egui;

#[test]
fn test_client_message_variants_roundtrip() {
    let rect_shape = Shape::new_rect(101, egui::Rect::from_min_size(egui::pos2(10.0, 20.0), egui::vec2(100.0, 50.0)), egui::Color32::BLUE, 2.0, true);

    let msgs = vec![
        ClientMessage::Authenticate { token: "token_123".to_string() },
        ClientMessage::JoinRoom { room_code: "room_abc".to_string() },
        ClientMessage::LeaveRoom,
        ClientMessage::CatchUp { last_seen_seq: 42 },
        ClientMessage::CursorMove { x: 10.5, y: 20.25, selected_ids: vec![1, 2] },
        ClientMessage::RequestLock { shape_id: 101 },
        ClientMessage::ReleaseLock { shape_id: 101 },
        ClientMessage::CreateShape { shape: rect_shape.clone() },
        ClientMessage::UpdateShape { shape_id: 101, data: rect_shape.data.clone() },
        ClientMessage::DeleteShapes { shape_ids: vec![101, 102] },
        ClientMessage::ReorderShapes { shape_ids: vec![101], action: ZOrderAction::BringToFront },
        ClientMessage::Ping,
    ];

    for msg in msgs {
        let bytes = msg.to_msgpack().expect("MessagePack serialization failed");
        let decoded = ClientMessage::from_msgpack(&bytes).expect("MessagePack deserialization failed");
        assert_eq!(msg.to_json().unwrap(), decoded.to_json().unwrap());
    }
}

#[test]
fn test_server_message_variants_roundtrip() {
    let user = RemoteUser {
        id: "usr_1".to_string(),
        display_name: "Falcon".to_string(),
        color: [100, 200, 255, 255],
        avatar_url: Some("https://example.com/avatar.png".to_string()),
    };

    let msgs = vec![
        ServerMessage::UserJoined { user: user.clone() },
        ServerMessage::UserLeft { user_id: "usr_1".to_string() },
        ServerMessage::RemoteCursor { user_id: "usr_1".to_string(), x: 50.0, y: 75.0, selected_ids: vec![1] },
        ServerMessage::LockGranted { shape_id: 10, user_id: "usr_1".to_string() },
        ServerMessage::LockDenied { shape_id: 10, owner_id: "usr_2".to_string() },
        ServerMessage::LockReleased { shape_id: 10 },
        ServerMessage::Pong,
        ServerMessage::Error { message: "Test error".to_string() },
    ];

    for msg in msgs {
        let bytes = msg.to_msgpack().expect("MessagePack serialization failed");
        let decoded = ServerMessage::from_msgpack(&bytes).expect("MessagePack deserialization failed");
        assert_eq!(msg.to_json().unwrap(), decoded.to_json().unwrap());
    }
}
