# Kugel File Management & Cloud Sync Architecture Plan

## Executive Summary
This specification defines the file storage, cloud sync, transition pipeline, security architecture, and user interface for **Kugel Local & Cloud Boards**. 

It guarantees **Local-First Data Preservation**: local `.kugel` files remain completely standalone and private, while cloud `.kugelsh` files serve as lightweight pointers allowing live multi-user collaboration with zero risk of data loss.

---

## 1. File Format Specifications

### 1.1 Local Board (`.kugel`)
Self-contained JSON file containing full canvas state, background color, viewport pan/zoom, and embedded compressed base64 images.

```json
{
  "version": "1.0",
  "shapes": [ ... ],
  "background_color": [20, 20, 23, 255],
  "zoom": 1.0,
  "pan_offset": [0.0, 0.0],
  "next_id": 42,
  "dark_mode": true
}
```

### 1.2 Cloud Shared Board Pointer (`.kugelsh`)
Portable JSON file that acts as a room pointer with an embedded `offline_snapshot` for offline reading.

```json
{
  "version": "1.0",
  "kugelsh_schema": "1.0",
  "sync": {
    "room_id": "kugel-room-uuid-v4",
    "server_url": "wss://api.kugel.app/v1/sync",
    "public_share_token": "token_for_guest_access"
  },
  "metadata": {
    "title": "Project Moodboard",
    "created_at": "2026-07-30T21:35:00Z",
    "last_synced_at": "2026-07-30T21:35:00Z"
  },
  "offline_snapshot": { ... full CanvasState for offline viewing ... }
}
```

> [!CAUTION]
> **Security Rule**: Auth tokens and secret user edit keys MUST NEVER be written into the `.kugelsh` file! Users frequently share `.kugelsh` files via Email or Slack. User session credentials must reside in OS Keychain / Local Secure Storage (`~/.local/share/kugel/credentials.json`).

---

## 2. Storage Semantics & Edge-Case Mitigation

```
                  ┌───────────────────────────────────────────────┐
                  │                 KUGEL CLIENT                  │
                  └──────┬────────────────────────────────┬───────┘
                         │                                │
            1. WS Sync   │                                │ 2. Local Disk
            (Realtime)   │                                │ (Save / Cache)
                         ▼                                ▼
              ┌─────────────────────┐          ┌─────────────────────┐
              │    KUGEL SERVER     │          │  Local App Cache    │
              │  Authoritative State│          │ ~/.local/.../cache/ │
              └─────────────────────┘          └─────────────────────┘
                                                          │ 3. Explicit Save / Exit
                                                          ▼
                                               ┌─────────────────────┐
                                               │   .kugelsh File     │
                                               │  (Offline Snapshot) │
                                               └─────────────────────┘
```

### 2.1 The "Cloud Drive Syncing" Loop Fix
- **Problem**: Writing to `.kugelsh` on every live 60Hz mouse movement causes iCloud/Dropbox to constantly re-sync the file, generating `board (Conflicted copy).kugelsh`.
- **Solution**: Live updates write strictly to **In-Memory State** and the local internal cache (`~/.local/share/kugel/cache/<room_id>.json`). The `.kugelsh` file on disk is only updated when the user explicitly presses `Cmd/Ctrl+S` or exits the app.

### 2.2 Asset Optimization (Content-Addressable Storage - CAS)
- Streaming raw base64 images over WebSocket degrades performance and causes Head-of-Line blocking.
- **Image Upload Pipeline**:
  1. Image added to canvas -> Client calculates `SHA-256` hash of compressed image bytes.
  2. Client checks if Server already has `SHA-256` hash.
  3. If missing -> Upload image binary payload once via HTTP POST `/api/v1/assets`.
  4. Sync message sends lightweight image reference `{ asset_hash: "sha256:...", width, height }`.

### 2.3 Transition Workflows (Zero Data Loss)

```
[ Local .kugel Board ] ──( Publish to Cloud )──► [ Server Room Created ]
                                                        │
                                                        ▼
[ Original .kugel Saved to Backup ] ◄──── [ .kugelsh Pointer File Created ]
```

1. **Publish (Local `.kugel` -> Cloud `.kugelsh`)**:
   - Original `.kugel` file is backed up to `~/.local/share/kugel/backups/<timestamp>_<name>.kugel`.
   - Client creates server room, uploads shapes + images.
   - Current file path switches to `name.kugelsh`.
2. **Make Local Copy (Cloud `.kugelsh` -> Local `.kugel`)**:
   - Takes current live canvas state from memory.
   - Prompts native save file dialog (`rfd`) for `.kugel`.
   - File opens as independent, local-only `.kugel` board.
3. **Unpublish (Destroy Cloud Room)**:
   - Requires room owner permissions.
   - Sends server teardown command.
   - Converts active `.kugelsh` file in-place back to standalone `.kugel` file.

---

## 3. UI Dashboard & Desktop Integration

### 3.1 Recent Boards & Dashboard Modal (`egui`)
- Accessible via **Home** icon in top-left bar or on fresh app launch.
- Displays grid of recent files sorted by last opened time:
  - **📄 Local Badge**: Indicates offline `.kugel` file.
  - **☁️ Cloud Badge**: Indicates `.kugelsh` live shared board.
  - **Thumbnail Preview**: Mini canvas preview or last snapshot thumbnail.

### 3.2 Top Bar Controls & Status Badges
- **Status Indicator**:
  - 🟢 **Live**: Connected to WebSocket, real-time sync active.
  - 🟡 **Syncing / Reconnecting**: Connecting or backoff retry.
  - 🔴 **Offline**: Connection down; using local cached snapshot.
- **Active User Avatars**: Displays overlapping user avatar pills with active user names on hover.

### 3.3 macOS & Desktop Integration
- **Double-click `.kugelsh`**:
  - Handled via `macos_open.rs` (`kAEOpenDocuments` Apple Event) and CLI args.
  - App opens, reads `.kugelsh` JSON metadata, immediately establishes WebSocket session, and syncs room state.
- **Unsaved Changes Shield**: Intercepts file open events; if local unsaved edits exist, prompts user to save before switching boards.
