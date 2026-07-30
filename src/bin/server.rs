use kugel::server::KugelServer;
use std::net::SocketAddr;
use std::path::PathBuf;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let port = std::env::var("PORT")
        .ok()
        .and_then(|p| p.parse().ok())
        .unwrap_or(8765);

    let host = std::env::var("HOST").unwrap_or_else(|_| "0.0.0.0".to_string());
    let db_path = std::env::var("DATABASE_PATH")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from("./data/kugel.db"));

    let addr: SocketAddr = format!("{host}:{port}").parse()?;
    println!("Starting Kugel Collaboration Server on {addr}");

    let server = match KugelServer::new_with_db(&db_path) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("Failed to open DB at {:?}, falling back to in-memory: {e}", db_path);
            KugelServer::new_in_memory()?
        }
    };

    server.run(addr).await.map_err(|e| e.into())
}
