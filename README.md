# Kugel

A fast, minimalist mood board for the desktop. Drop images, sketch, drag things around, and lay out ideas on an infinite canvas. Built in Rust with egui and Skia.

## Features

- Infinite canvas with pan, zoom, and an optional dot grid
- Shapes: pen, rectangle, circle, text, sticky notes, and images
- Section boxes: outline frames that stay out of the way. Their interior is click through, so you can still grab shapes underneath, and only the border is selectable
- Select, move, and resize freely, with multi select and group resize
- Alt and drag to duplicate
- Alignment guides that snap edges and centers without sticking or drifting
- Paste images straight from the clipboard, or drag and drop image files
- Undo and redo
- Save and load boards as `.kugel` files
- Export the canvas to PNG or JPEG at a chosen resolution
- Dark and light themes that follow the system setting

## Install

Requires a recent Rust toolchain.

```bash
git clone https://github.com/salernoelia/kugel
cd kugel
cargo run --release
```

The release build is tuned for speed, so prefer `--release` for daily use.

## Controls

### Mouse and trackpad

- Trackpad two finger scroll pans the canvas
- Mouse wheel zooms toward the cursor
- Cmd or Ctrl and scroll zooms on any device
- Pinch zooms
- Middle click drag, or hold Space and drag, to pan
- Drag on empty space to marquee select
- Drag a corner handle to resize, drag the group box to resize a selection together

### Tools

| Key | Tool |
| --- | --- |
| V | Select |
| P | Pen |
| R | Rectangle |
| O | Circle |
| T | Text |
| N | Sticky note |
| F | Section box |
| I | Import image |

### Editing

| Shortcut | Action |
| --- | --- |
| Cmd/Ctrl + C | Copy |
| Cmd/Ctrl + V | Paste (shape, image, or text) |
| Cmd/Ctrl + D | Duplicate |
| Cmd/Ctrl + Z | Undo |
| Cmd/Ctrl + Y | Redo |
| Cmd/Ctrl + S | Save |
| Cmd/Ctrl + O | Open |
| Cmd/Ctrl + E | Export |
| Arrow keys | Nudge selection (hold Shift for larger steps) |
| Delete / Backspace | Delete selection |

Double click a text or sticky note to edit it.

## File format

Boards are plain JSON with a `.kugel` extension. They store shapes, background color, view state, and embedded image data, so a board file is self contained.

## License

MIT
