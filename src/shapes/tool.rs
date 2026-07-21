use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Tool {
    Select,
    Pen,
    Line,
    Rectangle,
    Circle,
    Text,
    StickyNote,
    Section,
}
