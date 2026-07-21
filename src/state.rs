use crate::shapes::Shape;
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize)]
pub struct CanvasState {
    pub version: String,
    pub shapes: Vec<Shape>,
    pub background_color: [u8; 4],
    pub zoom: f32,
    pub pan_offset: [f32; 2],
    pub next_id: usize,
    #[serde(default = "default_true")]
    pub dark_mode: bool,
}

fn default_true() -> bool {
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_canvas_state_default_true() {
        assert!(default_true());
        let json = r#"{"version":"1.0","shapes":[],"background_color":[0,0,0,255],"zoom":1.0,"pan_offset":[0.0,0.0],"next_id":0}"#;
        let state: CanvasState = serde_json::from_str(json).unwrap();
        assert!(state.dark_mode);
    }
}
