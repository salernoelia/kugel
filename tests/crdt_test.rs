use kugel::net::crdt::{DeltaRingBuffer, FractionalZIndex, LocalDelta, SelectiveUndoEngine};
use kugel::net::protocol::ServerMessage;
use kugel::shapes::Shape;
use std::collections::HashMap;

#[test]
fn test_fractional_z_index_ordering() {
    let key1 = FractionalZIndex::generate_between(None, None);
    let key2 = FractionalZIndex::generate_between(Some(&key1), None);
    let key3 = FractionalZIndex::generate_between(Some(&key1), Some(&key2));
    let key4 = FractionalZIndex::generate_between(None, Some(&key1));

    assert!(key4 < key1);
    assert!(key1 < key3);
    assert!(key3 < key2);

    let shape_a = Shape::new_rect(1, eframe::egui::Rect::EVERYTHING, eframe::egui::Color32::RED, 1.0, false);
    let shape_b = Shape::new_rect(2, eframe::egui::Rect::EVERYTHING, eframe::egui::Color32::GREEN, 1.0, false);
    let mut shapes = vec![shape_a.clone(), shape_b.clone()];

    let mut z_map = HashMap::new();
    z_map.insert(1, key2.clone()); // shape_a is higher
    z_map.insert(2, key1.clone()); // shape_b is lower

    FractionalZIndex::sort_shapes(&mut shapes, &z_map);
    assert_eq!(shapes[0].id, 2);
    assert_eq!(shapes[1].id, 1);
}

#[test]
fn test_delta_ring_buffer_eviction_and_replay() {
    let mut ring = DeltaRingBuffer::new(3);

    ring.push(10, ServerMessage::Pong);
    ring.push(11, ServerMessage::Pong);
    ring.push(12, ServerMessage::Pong);

    // Replay deltas since seq 10 -> returns seq 11 and 12
    let deltas = ring.get_deltas_since(10).expect("Should have deltas");
    assert_eq!(deltas.len(), 2);
    assert_eq!(deltas[0].0, 11);
    assert_eq!(deltas[1].0, 12);

    // Push seq 13, evicting 10
    ring.push(13, ServerMessage::Pong);

    // Asking for seq 9 (evicted) returns None
    assert!(ring.get_deltas_since(9).is_none());
}

#[test]
fn test_selective_undo_engine() {
    let mut engine = SelectiveUndoEngine::default();

    let shape = Shape::new_rect(1, eframe::egui::Rect::EVERYTHING, eframe::egui::Color32::WHITE, 1.0, false);
    engine.push_delta(LocalDelta::Create { shape: shape.clone() });

    let delta = engine.pop_undo().expect("Undo item expected");
    if let LocalDelta::Create { shape: s } = delta {
        assert_eq!(s.id, 1);
    } else {
        panic!("Mismatch delta type");
    }
}
