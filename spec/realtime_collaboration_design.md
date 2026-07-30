# Kugel Real-Time Collaboration Architecture & Design Specification

## Executive Summary
This document specifies the design for **Kugel Real-Time Collaboration**, a sub-50ms server-hosted collaboration system for the Kugel desktop mood board application. It enables multi-user infinite canvas editing with real-time cursor presence, object-level pessimistic/optimistic locking, instant delta synchronization, and frictionless user authentication & room management.

---

## 1. System Architecture & Topology

```
                  ┌───────────────────────────────────────────────┐
                  │                 KUGEL CLIENT                  │
                  │   (Rust / egui / eframe / Skia Canvas)        │
                  └───────┬───────────────────────────────▲───────┘
                          │                               │
            WebSocket /   │ Binary WebSocket             │ State Updates
            Presence      │ (MessagePack / Protobuf)      │ & Cursors
                          ▼                               │
                  ┌───────────────────────────────────────────────┐
                  │            KUGEL SERVER (AUTHORITATIVE)       │
                  │   Rust (Tokio + Axum + Tokio-Tungstenite)     │
                  │                                               │
                  │  ┌──────────────┐    ┌─────────────────────┐  │
                  │  │ Room Router  │    │  Object Lock Manager│  │
                  │  └──────┬───────┘    └──────────┬──────────┘  │
                  │         │                       │             │
                  │  ┌──────▼───────────────────────▼──────────┐  │
                  │  │  In-Memory Room State (DashMap)         │  │
                  │  └──────────────────────┬──────────────────┘  │
                  └─────────────────────────┼─────────────────────┘
                                            │ Periodic Snapshots
                                            ▼
                  ┌───────────────────────────────────────────────┐
                  │     PERSISTENCE (SQLite / PostgreSQL)         │
                  │        Users, Rooms, Snapshots (.kugel)       │
                  └───────────────────────────────────────────────┘
```

### Why Server-Hosted over Peer-to-Peer (P2P)?
1. **NAT Traversal & Reliability**: P2P (WebRTC data channels) requires complex STUN/TURN infrastructure, frequently fails behind enterprise firewalls/Symmetric NATs, and scales poorly as peer count increases (\(O(N^2)\) meshes).
2. **Authoritative Conflict Resolution & Locking**: Server hosting provides a single source of truth for object locking, sequence numbers, and atomic shape operations, completely avoiding split-brain conflicts.
3. **Persistence & Offline Reconnects**: Users joining a room get instant full-state sync directly from the server memory without requiring an existing peer to be online and serving bandwidth.

---

## 2. Technology Stack Selection

| Component | Selected Stack | Justification |
| :--- | :--- | :--- |
| **Backend Framework** | **Rust** (`axum` + `tokio`) | Max performance, zero GC pauses, sub-millisecond serialization. Shares native Rust data models (`Shape`, `ShapeData`, `Color32`) directly with the `kugel` client crate. |
| **Real-time Transport** | **WebSocket** (`tokio-tungstenite`) | Low overhead framing, full duplex, broad firewall compatibility. Upgrade path to WebTransport (QUIC) for zero head-of-line blocking on cursor streams. |
| **Serialization** | **MessagePack** (`rmp-serde` struct-as-map) or **Protobuf** | Safe forward/backward schema compatibility (`#[serde(default)]`), ignoring unknown fields across client versions. |
| **Database & Auth** | **SQLite** (`sqlx`) or **PostgreSQL** + **JWT / Argon2id** | Lightweight, zero-config deployment for self-hosting (SQLite) or production cloud scale (Postgres). Argon2id for password hashing, short-lived JWT for socket authorization. |
| **In-Memory State** | **DashMap / Tokio RwLock** | Lock-free concurrent read/write access for active room sessions serving 60Hz tick rates per user. |

---

## 3. Real-Time Sync & Object Locking Protocol

### 3.1 Object Locking Model (Pessimistic + Optimistic)
To prevent "tug-of-war" editing (two users moving/editing the same shape simultaneously):
- **Lock Acquisition**: When a user clicks or starts dragging a shape, the client immediately sends `LockShape { shape_id }`.
- **Server Verification**:
  - If un-locked or owned by requesting user -> Server confirms `LockGranted { shape_id, user_id }` and broadcasts lock state to room.
  - If locked by another user -> Server returns `LockDenied { shape_id, owner_id }`. The requesting client rejects selection visually.
- **Lock Auto-Release**:
  - Sent explicitly on `MouseUp` / deselect via `UnlockShape { shape_id }`.
  - Automatically released after **3000ms heartbeat timeout** if client disconnects or freezes.

### 3.2 Protocol Messages (Serde MessagePack Enums)

```rust
#[derive(Serialize, Deserialize, Debug, Clone)]
pub enum ClientMessage {
    // Auth & Room
    Authenticate { token: String },
    JoinRoom { room_code: String },
    LeaveRoom,

    // Reconnection Catch-Up
    CatchUp { last_seen_seq: u64 },

    // High-frequency Ephemeral (~30-60Hz)
    CursorMove { x: f32, y: f32, selected_ids: Vec<u64> },

    // Shape Mutation & Locking
    RequestLock { shape_id: u64 },
    ReleaseLock { shape_id: u64 },
    UpdateShape { shape_id: u64, data: ShapeData },
    CreateShape { shape: Shape },
    DeleteShapes { shape_ids: Vec<u64> },
    ReorderShapes { shape_ids: Vec<u64>, action: ZOrderAction },

    // Heartbeat
    Ping,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub enum ServerMessage {
    // Room Session Setup
    RoomState {
        room_code: String,
        users: Vec<RemoteUser>,
        shapes: Vec<Shape>,
        locked_shapes: HashMap<u64, String>, // ShapeId -> UserId
        current_seq: u64,
    },
    UserJoined { user: RemoteUser },
    UserLeft { user_id: String },

    // Real-Time Events
    RemoteCursor { user_id: String, x: f32, y: f32, selected_ids: Vec<u64> },
    LockGranted { shape_id: u64, user_id: String },
    LockDenied { shape_id: u64, owner_id: String },
    LockReleased { shape_id: u64 },
    ShapeUpdated { user_id: String, shape_id: u64, data: ShapeData, seq: u64 },
    ShapeCreated { user_id: String, shape: Shape, seq: u64 },
    ShapesDeleted { user_id: String, shape_ids: Vec<u64>, seq: u64 },

    // System
    Pong,
    Error { message: String },
}
```

---

## 4. User Accounts & Frictionless Management

### 4.1 Frictionless Authentication Flow
1. **Instant Anonymous / Guest Session**:
   - Users can join any shared link (`kugel://room/XYZ` or `kugel.app/join/XYZ`) instantly without signing up.
   - Server issues a guest JWT with a randomized name (e.g. "Guest Falcon", "Guest Otter") and assigned cursor color.
2. **Simple Registered Accounts**:
   - Quick Signup: Email + Password or Passkey / OAuth (Google/GitHub).
   - User profile stores: `user_id`, `email`, `display_name`, `avatar_url`, `preferred_color`.
3. **Room Permission Models**:
   - **Public (Link sharing)**: Anyone with link can view/edit.
   - **Protected**: Password required or view-only mode for non-owners.

---

## 5. Client Integration Architecture

### 5.1 Non-Blocking Network Pipeline (`src/net/`)

```
Tokio Network Thread (WS Loop) ──(crossbeam channel)──► egui UI Thread (eframe)
                                                        │
                                                        ▼
                                             Render Loop (60/120 Hz)
```

- **Lock-Free Message Passing**: The async Tokio task manages WebSocket communication and pushes received delta messages into a `crossbeam_channel::unbounded()` queue.
- **Wake Up Signal**: Tokio task triggers `egui_ctx.request_repaint()`.
- **UI Thread Consumption**: Inside `eframe::update()`, the main UI thread calls `receiver.try_iter().collect()` to drain pending deltas non-blockingly, applying updates to canvas state without frame drops.
