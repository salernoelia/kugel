use crate::net::kugelsh::CasAsset;
use crate::server::state::ServerState;
use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::Json;
use serde::{Deserialize, Serialize};

#[derive(Deserialize)]
pub struct CreateRoomRequest {
    pub title: Option<String>,
}

#[derive(Serialize)]
pub struct CreateRoomResponse {
    pub room_id: String,
    pub share_token: String,
    pub title: String,
}

pub async fn create_room_handler(
    State(state): State<ServerState>,
    Json(req): Json<CreateRoomRequest>,
) -> impl IntoResponse {
    let room_id = uuid::Uuid::new_v4().to_string();
    let share_token = uuid::Uuid::new_v4().to_string();
    let title = req.title.unwrap_or_else(|| "Untitled Mood Board".to_string());

    let _ = state.get_or_create_room(&room_id, &title).await;

    Json(CreateRoomResponse {
        room_id,
        share_token,
        title,
    })
}

pub async fn upload_asset_handler(
    State(state): State<ServerState>,
    body_bytes: axum::body::Bytes,
) -> Result<impl IntoResponse, (StatusCode, String)> {
    if body_bytes.is_empty() {
        return Err((StatusCode::BAD_REQUEST, "Empty asset body".to_string()));
    }
    if body_bytes.len() > 5 * 1024 * 1024 {
        return Err((StatusCode::PAYLOAD_TOO_LARGE, "Asset exceeds 5MB limit".to_string()));
    }

    let hash = CasAsset::compute_sha256(&body_bytes);
    state
        .db
        .save_asset(&hash, &body_bytes)
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e))?;

    Ok(Json(serde_json::json!({
        "hash": hash,
        "size": body_bytes.len()
    })))
}

pub async fn get_asset_handler(
    State(state): State<ServerState>,
    Path(hash): Path<String>,
) -> Result<impl IntoResponse, (StatusCode, String)> {
    let asset = state
        .db
        .load_asset(&hash)
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e))?;

    if let Some(bytes) = asset {
        Ok((
            [
                (axum::http::header::CONTENT_TYPE, "application/octet-stream"),
                (axum::http::header::CACHE_CONTROL, "public, max-age=31536000, immutable"),
            ],
            bytes,
        ))
    } else {
        Err((StatusCode::NOT_FOUND, "Asset not found".to_string()))
    }
}
