# Kugel

![Demo](./assets/demo.png)

A fast, minimalist white-board for the desktop. Drop images, sketch, drag things around, and lay out ideas on an infinite canvas. Built in Rust with egui and Skia.

## Why?

- Speed
- File-based
- Desktop

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
cargo bundle --release
```

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
| V or W | Select |
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

Double click a text or sticky note to edit it. If a text is nothing but a link, Cmd/Ctrl + click it to open it in your browser.

The most recently opened board reopens automatically on the next launch.

## File format

Boards are plain JSON with a `.kugel` extension. They store shapes, background color, view state, and embedded image data, so a board file is self contained.

## macOS file association

To make `.kugel` files open with Kugel on double click (and show the Kugel icon), install the app bundle with:

```bash
./packaging/macos/install.sh
```

This builds `Kugel.app`, injects the `.kugel` document type into `Info.plist` (which `cargo bundle` omits), copies it to `/Applications`, and registers it with Launch Services.

## License

MIT
