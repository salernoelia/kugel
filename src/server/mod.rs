pub mod api;
pub mod locks;
pub mod persistence;
pub mod state;
pub mod ws;

use api::{create_room_handler, get_asset_handler, upload_asset_handler};
use axum::routing::{get, post};
use axum::Router;
use persistence::ServerDb;
use state::ServerState;
use std::net::SocketAddr;
use tokio::net::TcpListener;

#[derive(Clone)]
pub struct KugelServer {
    pub state: ServerState,
}

impl KugelServer {
    pub fn new_in_memory() -> Result<Self, String> {
        let db = ServerDb::memory()?;
        let state = ServerState::new(db);
        Ok(Self { state })
    }

    pub fn new_with_db(db_path: &std::path::PathBuf) -> Result<Self, String> {
        let db = ServerDb::open_file(db_path)?;
        let state = ServerState::new(db);
        Ok(Self { state })
    }

    pub fn router(&self) -> Router {
        Router::new()
            .route("/api/v1/rooms", post(create_room_handler))
            .route("/api/v1/assets", post(upload_asset_handler))
            .route("/api/v1/assets/:hash", get(get_asset_handler))
            .route("/v1/sync/:room_id", get(ws::ws_handler))
            .with_state(self.state.clone())
    }

    pub async fn run(&self, addr: SocketAddr) -> Result<(), String> {
        let listener = TcpListener::bind(addr)
            .await
            .map_err(|e| format!("Failed to bind to {addr}: {e}"))?;

        axum::serve(listener, self.router())
            .await
            .map_err(|e| format!("Server execution error: {e}"))
    }

    pub fn spawn_in_background(&self, addr: SocketAddr) {
        let server = self.clone();
        std::thread::spawn(move || {
            let rt = match tokio::runtime::Builder::new_current_thread().enable_all().build() {
                Ok(r) => r,
                Err(e) => {
                    eprintln!("Failed to create tokio runtime for background server: {e}");
                    return;
                }
            };
            rt.block_on(async move {
                if let Err(e) = server.run(addr).await {
                    eprintln!("Background server error: {e}");
                }
            });
        });
    }
}
