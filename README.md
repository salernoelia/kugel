# Kugel

![Demo](./assets/demo.png)

A fast, minimalist mood board for the desktop. Drop images, render PDF pages, sketch, and lay out ideas on an infinite canvas. Built in Rust with egui and Skia.

## Features

- Infinite canvas with pan, zoom, and optional dot grid
- Shapes: pen, line, rectangle, circle, text, sticky notes, and images
- Section boxes: outline frames with click-through interiors so shapes inside remain selectable
- PDF import: drag and drop PDF files to render all pages as a row of images
- Image support: paste images from clipboard, drag and drop files, or import via file picker
- Automatic link previews: extracts web links, fetches page titles in the background, and opens links with Cmd/Ctrl + click
- Selection and transform: single select, marquee multi-select, Shift + click toggle, group resize, and Alt + drag duplicate
- Declutter tool: Shift + A arranges selected elements into a neat horizontal row with uniform top alignment and spacing
- Alignment snapping: smart guides snap edges and centers of moving shapes to existing elements
- Full undo and redo history for all actions including drawing, moves, resizes, text edits, and decluttering
- Save and load board state as self-contained `.kugel` files
- Export canvas to PNG or JPEG at scalable resolutions
- Dark and light themes that match system settings automatically
- Automatic update checker and in-app updater

## Install

Requires a recent Rust toolchain.

```bash
git clone https://github.com/salernoelia/kugel
cd kugel
cargo bundle --release
```

## Controls

### Mouse and Trackpad

- Trackpad two-finger scroll to pan
- Mouse wheel to zoom toward cursor
- Cmd or Ctrl + scroll to zoom on any device
- Pinch to zoom
- Middle click drag, or hold Space and drag, to pan
- Drag on empty space for marquee selection
- Drag corner handle to resize a shape or group selection

### Tools

| Key | Tool |
| --- | --- |
| V or W | Select |
| P | Pen |
| L | Line |
| R | Rectangle |
| O | Circle |
| T | Text |
| N | Sticky note |
| F | Section box |
| I | Import image |

### Editing Shortcuts

| Shortcut | Action |
| --- | --- |
| Cmd/Ctrl + C | Copy |
| Cmd/Ctrl + V | Paste (shape, image, or text) |
| Cmd/Ctrl + D | Duplicate selection |
| Shift + A | Declutter selection into a row |
| Cmd/Ctrl + Z | Undo |
| Cmd/Ctrl + Y | Redo |
| Cmd/Ctrl + S | Save board |
| Cmd/Ctrl + O | Open board |
| Cmd/Ctrl + E | Export image |
| Arrow keys | Nudge selection (hold Shift for larger steps) |
| Delete / Backspace | Delete selection |

Double click a text or sticky note to edit it.

## File Format

Boards are stored as plain JSON files with a `.kugel` extension. They store shapes, background color, view settings, and base64-encoded image data so board files are completely self-contained.

## macOS File Association

To associate `.kugel` files with Kugel on double click, run:

```bash
./packaging/macos/install.sh
```

## License

MIT
