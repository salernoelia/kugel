use kugel::net::kugelsh::{CasAsset, KugelCloudPointer, LocalRoomCache};
use kugel::state::CanvasState;

#[test]
fn test_atomic_file_save_and_cloud_pointer() {
    let temp_dir = tempfile::tempdir().expect("tempdir");
    let file_path = temp_dir.path().join("cloud_board.kugelsh");

    let state = CanvasState {
        version: "1.0".to_string(),
        shapes: vec![],
        background_color: [20, 20, 23, 255],
        zoom: 1.5,
        pan_offset: [100.0, -50.0],
        next_id: 10,
        dark_mode: true,
    };

    let pointer = KugelCloudPointer::new(
        "room_99".to_string(),
        "wss://api.kugel.app/v1/sync".to_string(),
        "share_token_123".to_string(),
        "My Project".to_string(),
        state,
    );

    pointer.save_atomic(&file_path).expect("Atomic save failed");
    assert!(file_path.exists());
    assert!(!temp_dir.path().join("cloud_board.kugelsh.tmp").exists()); // tmp file cleaned up

    let loaded = KugelCloudPointer::read_from_file(&file_path).expect("Read failed");
    assert_eq!(loaded.sync.room_id, "room_99");
    assert_eq!(loaded.offline_snapshot.zoom, 1.5);
}

#[test]
fn test_local_room_cache_write_and_read() {
    let room_id = "test_cache_room_42";
    let state = CanvasState {
        version: "1.0".to_string(),
        shapes: vec![],
        background_color: [0, 0, 0, 255],
        zoom: 2.0,
        pan_offset: [0.0, 0.0],
        next_id: 5,
        dark_mode: true,
    };

    LocalRoomCache::save_cache(room_id, &state).expect("Cache save failed");
    let loaded = LocalRoomCache::load_cache(room_id).expect("Cache read failed");
    assert_eq!(loaded.zoom, 2.0);
    assert_eq!(loaded.next_id, 5);
}

#[test]
fn test_cas_sha256_asset_storage() {
    let binary_data = b"Precious image pixel bytes 0xDEADBEEF";
    let hash = CasAsset::compute_sha256(binary_data);

    let path = CasAsset::save_local(&hash, binary_data).expect("CAS save failed");
    assert!(path.exists());

    let loaded = CasAsset::load_local(&hash).expect("CAS load failed");
    assert_eq!(loaded, binary_data);
}
