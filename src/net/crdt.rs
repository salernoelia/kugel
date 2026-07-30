use crate::net::protocol::ServerMessage;
use crate::shapes::{Shape, ShapeData};
use std::collections::{HashMap, VecDeque};

/// Fractional Z-Index Key Generator for conflict-free shape ordering
#[derive(Debug, Clone)]
pub struct FractionalZIndex;

impl FractionalZIndex {
    /// Generate a key between `prev` and `next`.
    /// If `prev` is None, key comes before `next`.
    /// If `next` is None, key comes after `prev`.
    pub fn generate_between(prev: Option<&str>, next: Option<&str>) -> String {
        let p = prev.unwrap_or("");
        let n = next.unwrap_or("");

        if p.is_empty() && n.is_empty() {
            return "m".to_string();
        }
        if p.is_empty() {
            if let Some(first_char) = n.chars().next() {
                if first_char > 'a' {
                    let prev_char = (first_char as u8 - 1) as char;
                    format!("{prev_char}")
                } else {
                    format!("a{n}")
                }
            } else {
                "a".to_string()
            }
        } else if n.is_empty() {
            format!("{p}m")
        } else {
            if n.starts_with(p) {
                format!("{p}0m")
            } else {
                format!("{p}m")
            }
        }
    }

    /// Sort shapes deterministically by their z_index and shape ID tie-breaker
    pub fn sort_shapes(shapes: &mut [Shape], z_indices: &HashMap<usize, String>) {
        shapes.sort_by(|a, b| {
            let key_a = z_indices.get(&a.id).map(|s| s.as_str()).unwrap_or("");
            let key_b = z_indices.get(&b.id).map(|s| s.as_str()).unwrap_or("");
            key_a.cmp(key_b).then_with(|| a.id.cmp(&b.id))
        });
    }
}

/// Tombstone record for deleted shapes to prevent offline/online ghosting
#[derive(Debug, Clone, PartialEq)]
pub struct Tombstone {
    pub shape_id: usize,
    pub deleted_at_seq: u64,
    pub timestamp_ms: u64,
}

/// Selective Undo / Inverse Delta System
#[derive(Debug, Clone)]
pub enum LocalDelta {
    Create { shape: Shape },
    Update { shape_id: usize, old_data: ShapeData, new_data: ShapeData },
    Delete { shapes: Vec<Shape> },
    Reorder { shape_ids: Vec<usize>, old_order: Vec<usize> },
}

#[derive(Debug, Default)]
pub struct SelectiveUndoEngine {
    undo_stack: Vec<LocalDelta>,
    redo_stack: Vec<LocalDelta>,
}

impl SelectiveUndoEngine {
    pub fn push_delta(&mut self, delta: LocalDelta) {
        self.undo_stack.push(delta);
        self.redo_stack.clear();
    }

    pub fn pop_undo(&mut self) -> Option<LocalDelta> {
        self.undo_stack.pop()
    }

    pub fn push_redo(&mut self, delta: LocalDelta) {
        self.redo_stack.push(delta);
    }

    pub fn pop_redo(&mut self) -> Option<LocalDelta> {
        self.redo_stack.pop()
    }
}

/// In-Memory Delta Log Ring-Buffer for Reconnection Catch-Up Replay
#[derive(Debug)]
pub struct DeltaRingBuffer {
    capacity: usize,
    buffer: VecDeque<(u64, ServerMessage)>,
}

impl DeltaRingBuffer {
    pub fn new(capacity: usize) -> Self {
        Self {
            capacity,
            buffer: VecDeque::with_capacity(capacity),
        }
    }

    pub fn push(&mut self, seq: u64, msg: ServerMessage) {
        if self.buffer.len() >= self.capacity {
            self.buffer.pop_front();
        }
        self.buffer.push_back((seq, msg));
    }

    pub fn get_deltas_since(&self, last_seen_seq: u64) -> Option<Vec<(u64, ServerMessage)>> {
        if self.buffer.is_empty() {
            if last_seen_seq == 0 {
                return Some(Vec::new());
            }
            return None; // Evicted / clear state needed
        }

        let min_seq = self.buffer.front().unwrap().0;
        if last_seen_seq < min_seq && last_seen_seq > 0 {
            // Evicted! Server needs to send full room snapshot instead
            return None;
        }

        let deltas: Vec<_> = self
            .buffer
            .iter()
            .filter(|(seq, _)| *seq > last_seen_seq)
            .cloned()
            .collect();
        Some(deltas)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_fractional_z_index_generation() {
        let key1 = FractionalZIndex::generate_between(None, None);
        let key2 = FractionalZIndex::generate_between(Some(&key1), None);
        let key3 = FractionalZIndex::generate_between(Some(&key1), Some(&key2));

        assert!(key1 < key2);
        assert!(key1 < key3 && key3 < key2);
    }

    #[test]
    fn test_delta_ring_buffer_catchup() {
        let mut ring = DeltaRingBuffer::new(5);
        for i in 1..=5 {
            ring.push(i, ServerMessage::Pong);
        }

        // Request deltas since seq 2 -> should return seq 3, 4, 5
        let deltas = ring.get_deltas_since(2).expect("Should have deltas");
        assert_eq!(deltas.len(), 3);
        assert_eq!(deltas[0].0, 3);
        assert_eq!(deltas[2].0, 5);

        // Add 3 more (total 8), evicting 1, 2, 3
        for i in 6..=8 {
            ring.push(i, ServerMessage::Pong);
        }

        // Asking for last_seen_seq = 1 should return None (evicted)
        assert!(ring.get_deltas_since(1).is_none());
    }
}
