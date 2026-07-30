use crate::net::protocol::{ClientMessage, ServerMessage};
use eframe::egui;
use futures_util::{SinkExt, StreamExt};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Duration;
use tokio_tungstenite::connect_async;
use tokio_tungstenite::tungstenite::Message;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SyncStatus {
    Disconnected,
    Connecting,
    Live,
    Syncing,
    Error(String),
}

pub struct NetworkClient {
    pub room_id: String,
    pub server_url: String,
    pub token: String,
    pub tx_to_net: crossbeam_channel::Sender<ClientMessage>,
    pub rx_from_net: crossbeam_channel::Receiver<ServerMessage>,
    pub is_connected: Arc<AtomicBool>,
    pub last_seen_seq: Arc<AtomicU64>,
}

impl NetworkClient {
    pub fn connect(
        server_url: String,
        room_id: String,
        token: String,
        egui_ctx: egui::Context,
    ) -> Self {
        let (tx_to_net, rx_to_net) = crossbeam_channel::unbounded::<ClientMessage>();
        let (tx_from_net, rx_from_net) = crossbeam_channel::unbounded::<ServerMessage>();

        let is_connected = Arc::new(AtomicBool::new(false));
        let last_seen_seq = Arc::new(AtomicU64::new(0));

        let client = Self {
            room_id: room_id.clone(),
            server_url: server_url.clone(),
            token: token.clone(),
            tx_to_net,
            rx_from_net,
            is_connected: is_connected.clone(),
            last_seen_seq: last_seen_seq.clone(),
        };

        let is_conn_clone = is_connected.clone();
        let last_seq_clone = last_seen_seq.clone();

        std::thread::spawn(move || {
            let rt = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .expect("Failed to build Tokio runtime for network worker");

            rt.block_on(async move {
                let ws_url = format!("{server_url}/v1/sync/{room_id}");
                let mut backoff = Duration::from_millis(200);

                loop {
                    is_conn_clone.store(false, Ordering::SeqCst);
                    match connect_async(&ws_url).await {
                        Ok((ws_stream, _)) => {
                            is_conn_clone.store(true, Ordering::SeqCst);
                            backoff = Duration::from_millis(200);

                            let (mut ws_tx, mut ws_rx) = ws_stream.split();

                            // 1. Send Authenticate message
                            let auth_msg = ClientMessage::Authenticate {
                                token: token.clone(),
                            };
                            if let Ok(bytes) = auth_msg.to_msgpack() {
                                let _ = ws_tx.send(Message::Binary(bytes.into())).await;
                            }

                            // 2. Send CatchUp if reconnecting
                            let last_seq = last_seq_clone.load(Ordering::SeqCst);
                            if last_seq > 0 {
                                let catchup_msg = ClientMessage::CatchUp {
                                    last_seen_seq: last_seq,
                                };
                                if let Ok(bytes) = catchup_msg.to_msgpack() {
                                    let _ = ws_tx.send(Message::Binary(bytes.into())).await;
                                }
                            }

                            let tx_from_net_inner = tx_from_net.clone();
                            let rx_to_net_inner = rx_to_net.clone();
                            let ctx_inner = egui_ctx.clone();

                            // Channel to signal disconnect
                            let (close_tx, mut close_rx) = tokio::sync::oneshot::channel::<()>();

                            // Outgoing loop task
                            let send_task = tokio::spawn(async move {
                                loop {
                                    tokio::select! {
                                        _ = &mut close_rx => break,
                                        else => {
                                            if let Ok(msg) = rx_to_net_inner.try_recv() {
                                                if let Ok(bytes) = msg.to_msgpack() {
                                                    if ws_tx.send(Message::Binary(bytes.into())).await.is_err() {
                                                        break;
                                                    }
                                                }
                                            } else {
                                                tokio::time::sleep(Duration::from_millis(15)).await;
                                            }
                                        }
                                    }
                                }
                            });

                            // Incoming loop
                            while let Some(Ok(ws_msg)) = ws_rx.next().await {
                                let bytes = match ws_msg {
                                    Message::Binary(b) => b.to_vec(),
                                    Message::Text(t) => t.into_bytes(),
                                    Message::Ping(_) => continue,
                                    Message::Close(_) => break,
                                    _ => continue,
                                };

                                let server_msg = ServerMessage::from_msgpack(&bytes)
                                    .or_else(|_| ServerMessage::from_json(&String::from_utf8_lossy(&bytes)));

                                if let Ok(msg) = server_msg {
                                    // Update last_seen_seq if applicable
                                    match &msg {
                                        ServerMessage::RoomState { current_seq, .. } => {
                                            last_seq_clone.store(*current_seq, Ordering::SeqCst);
                                        }
                                        ServerMessage::ShapeUpdated { seq, .. }
                                        | ServerMessage::ShapeCreated { seq, .. }
                                        | ServerMessage::ShapesDeleted { seq, .. }
                                        | ServerMessage::ShapesReordered { seq, .. } => {
                                            last_seq_clone.fetch_max(*seq, Ordering::SeqCst);
                                        }
                                        _ => {}
                                    }

                                    let _ = tx_from_net_inner.send(msg);
                                    ctx_inner.request_repaint();
                                }
                            }

                            let _ = close_tx.send(());
                            send_task.abort();
                        }
                        Err(_) => {
                            tokio::time::sleep(backoff).await;
                            backoff = (backoff * 2).min(Duration::from_secs(5));
                        }
                    }
                }
            });
        });

        client
    }

    pub fn send(&self, msg: ClientMessage) {
        let _ = self.tx_to_net.send(msg);
    }

    pub fn poll_messages(&self) -> Vec<ServerMessage> {
        self.rx_from_net.try_iter().collect()
    }
}
