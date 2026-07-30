# Kugel Real-Time Collaboration & File System Master Architecture Specification

## Executive Summary
This document serves as the **Master Architecture Specification** for Kugel's Real-Time Collaboration System, `.kugel` / `.kugelsh` file formats, conflict resolution engine, security framework, and UI dashboard.

It synthesizes all findings, protocol designs, and edge-case mitigations produced during multi-subagent grilling rounds.

---

## Table of Contents
1. [System Architecture & Stack Selection](#1-system-architecture--stack-selection)
2. [Network Transport & Protocol](#2-network-transport--protocol)
3. [CRDT & Conflict Resolution Engine](#3-crdt--conflict-resolution-engine)
4. [File System: Local (.kugel) vs Cloud (.kugelsh)](#4-file-system-local-kugel-vs-cloud-kugelsh)
5. [Security & End-to-End Encryption (E2EE)](#5-security--end-to-end-encryption-e2ee)
6. [Desktop Graphics & Memory Performance](#6-desktop-graphics--memory-performance)
7. [UI Dashboard & Status Controls](#7-ui-dashboard--status-controls)
8. [Testing & Verification Suite](#8-testing--verification-suite)

---

## 1. System Architecture & Stack Selection

- **Topology**: Authoritative Server-Hosted Relay with In-Memory State (`DashMap`) and SQL persistence (`SQLite`/`PostgreSQL`).
- **Client Stack**: Rust (`egui` + `eframe` + `skia-safe`).
- **Server Stack**: Rust (`axum` + `tokio` + `tokio-tungstenite`).
- **Serialization**: MessagePack (`rmp-serde` with struct-as-map) for safe schema evolution and backward compatibility across client versions.

---

## 2. Network Transport & Protocol

### 2.1 Ephemeral vs Delta Channels
- **Ephemeral Presence**: Mouse cursor coordinates ($x, y$), active selections broadcast over lightweight ~30Hz channels without server persistence.
- **Persistent State Deltas**: Shape creations, transformations, color changes, and text edits assigned monotonically increasing Sequence Numbers (`SeqNo`).

### 2.2 Reconnection Protocol
- On WiFi/network drops, clients reconnect and issue `CatchUp { last_seen_seq }`.
- Server replays missing delta log entries from its in-memory ring-buffer.
- Full canvas snapshots are sent only if the client's `last_seen_seq` has been evicted from the ring-buffer.

### 2.3 Non-Blocking Tokio-to-egui Channel
- Background Tokio network worker communicates with `eframe` via an un-bounded `crossbeam_channel`.
- Tokio worker calls `egui_ctx.request_repaint()` to wake the main thread.
- `eframe::update()` drains incoming messages non-blockingly using `try_iter()`, preventing UI frame drops.

---

## 3. CRDT & Conflict Resolution Engine

### 3.1 Selective Undo (`Cmd+Z`)
- Local undo applies **inverse operation deltas** (e.g. $dx = -50$) strictly to the local user's operation history, preventing destruction of concurrent remote edits.

### 3.2 Tombstone Deletions
- Deleted shapes are assigned `deleted_at` timestamps instead of instant array removal to prevent "ghosting" when offline deletes merge with online edits.
- Tombstones are garbage collected once acknowledged by all active room peers.

### 3.3 Fractional Z-Index Indexing
- Shape ordering uses lexicographical string keys (e.g., `a0`, `a0.5`, `a1`) with client ID tie-breakers for deterministic layering without integer overflow or array-shift conflicts.

---

## 4. File System: Local (.kugel) vs Cloud (.kugelsh)

- **`.kugel`**: 100% offline self-contained JSON containing full state and embedded base64 image data.
- **`.kugelsh`**: Portable JSON cloud pointer containing `room_id`, `server_url`, `public_share_token`, and an `offline_snapshot`.
- **Cloud Drive Sync Protection**: Live real-time updates write to memory and `~/.local/share/kugel/cache/<room_id>.json`. The disk `.kugelsh` file is written ONLY on explicit Save (`Cmd+S`) or Exit to prevent iCloud/Dropbox conflicted copy loops.
- **Content-Addressable Asset Storage (CAS)**: Images are hashed via `SHA-256` and uploaded once via HTTP POST `/api/v1/assets`. Sync frames convey only lightweight hash references.

---

## 5. Security & End-to-End Encryption (E2EE)

- **Decoupled Auth Tokens**: Auth keys and user credentials are **never** stored inside `.kugelsh` files (preventing leaks when files are emailed). Credentials reside in the OS Keychain (`macOS Keychain` / `Windows Credential Manager`).
- **AES-GCM-256 E2EE**: Ephemeral pairwise session keys established via Double Ratchet algorithm with 96-bit random CSPRNG nonces per message.
- **Replay Protection**: Cleartext outer envelopes contain sequence numbers and timestamps verified before decryption.
- **Rate Limiting**: Server enforces token-bucket limits (50 msg/sec, 5MB frame size) to prevent "Shape Bombing" memory attacks.

---

## 6. Desktop Graphics & Memory Performance

- **Decoupled Render Loop**: Skia rendering loop runs independently at 60/120Hz using linear/cubic interpolation between network state updates (~20Hz).
- **Quad-Tree Spatial Invalidation**: Only dirty bounding rectangles of moving objects trigger redraw calls.
- **LRU VRAM Texture Streaming**: Remote images initially render low-resolution thumbnails; full 4K VRAM textures are loaded dynamically via LRU cache only when zoomed into view.

---

## 7. UI Dashboard & Status Controls

- **`egui` Dashboard Modal**: Displays Recent Boards grid with **📄 Local** vs **☁️ Cloud** badges.
- **Status Badge**: 🟢 **Live** / 🟡 **Syncing** / 🔴 **Offline** status indicators in top-left bar.
- **Active Users Presence**: Overlapping user avatar pills displaying remote participant names.
- **macOS Desktop Integration**: Native Apple Event (`kAEOpenDocuments`) handling in `macos_open.rs` for `.kugelsh` double-click launch with unsaved-changes protection.

---

## 8. Testing & Verification Suite

- **Atomic File Swaps**: Disk saves write to `.tmp` files before executing atomic renames to prevent corruption on crash/power loss.
- **Toxiproxy Chaos Tests**: Automated CI network tests simulating 500ms jitter, 10% packet drop, and sudden socket disconnection.
- **Headless Fuzz Testing**: Concurrent multi-client headless simulation verifying convergent canvas state and zero visual data loss.
