use crate::server::AppState;
use axum::body::Body;
use axum::extract::{Path, State};
use axum::http::{header, StatusCode};
use axum::response::{IntoResponse, Response};

pub(crate) async fn get_tile(
    State(state): State<AppState>,
    Path((z, x, y_with_ext)): Path<(u8, u32, String)>,
) -> impl IntoResponse {
    // Strip the .pbf extension
    let y = match y_with_ext.strip_suffix(".pbf") {
        Some(y_str) => match y_str.parse::<u32>() {
            Ok(y) => y,
            Err(_) => {
                log::warn!("Invalid y coordinate: {}", y_with_ext);
                return StatusCode::BAD_REQUEST.into_response();
            }
        },
        None => {
            log::warn!("Missing .pbf extension: {}", y_with_ext);
            return StatusCode::BAD_REQUEST.into_response();
        }
    };

    let tile_data = {
        // Get tile from PMTiles archive (acquire read lock)
        let collection = state.tile_collection.read().await;
        match collection.get_tile(z, x, y).await {
            Err(e) => {
                log::error!("Error reading tile {z}/{x}/{y}, error: {e}");
                return StatusCode::INTERNAL_SERVER_ERROR.into_response();
            }
            Ok(None) => {
                return StatusCode::NOT_FOUND.into_response();
            }
            Ok(Some(data)) => data,
        }
    };

    let mut response = Response::builder().status(StatusCode::OK);

    // TODO: support non-MVT tiles
    let content_type = "application/x-protobuf";
    response = response.header(header::CONTENT_TYPE, content_type);

    // TODO: support other tile_compression
    let tile_compression = "gzip";
    response = response.header(header::CONTENT_ENCODING, tile_compression);

    response.body(Body::from(tile_data)).unwrap()
}

// The rest of this module is a hack to stub out a proper tileserver by returning some fixed responses to
// resource requests.
// We should probably do something smarter and more dynamic, but this works for expediency.
const DEFAULT_STYLE_JSON: &str = include_str!("../../tileserver_styles/basic/style.json");
const DEFAULT_SPRITE_JSON: &str = include_str!("../../tileserver_styles/basic/sprite@2x.json");
const DEFAULT_SPRITE_PNG: &[u8] = include_bytes!("../../tileserver_styles/basic/sprite@2x.png");
const DEFAULT_TILE_JSON: &str = include_str!("../../tileserver_styles/basic/tile.json");
const DEFAULT_FONT: &[u8] =
    include_bytes!("../../tileserver_styles/fonts/Roboto%20Medium/0-255.pbf");

pub(crate) async fn get_default_style() -> impl IntoResponse {
    Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, "application/json")
        .body(Body::from(DEFAULT_STYLE_JSON))
        .unwrap()
}

pub(crate) async fn get_tile_json(State(_state): State<AppState>) -> impl IntoResponse {
    Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, "application/json")
        .body(Body::from(DEFAULT_TILE_JSON))
        .unwrap()
}

pub(crate) async fn get_sprite_json(State(_state): State<AppState>) -> impl IntoResponse {
    Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, "application/json")
        .body(Body::from(DEFAULT_SPRITE_JSON))
        .unwrap()
}

pub(crate) async fn get_sprite_png(State(_state): State<AppState>) -> impl IntoResponse {
    Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, "image/png")
        .body(Body::from(DEFAULT_SPRITE_PNG))
        .unwrap()
}

pub(crate) async fn get_font(
    State(_state): State<AppState>,
    Path((_font_stack, _range_with_ext)): Path<(String, String)>,
) -> impl IntoResponse {
    Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, "application/json")
        .body(Body::from(DEFAULT_FONT))
        .unwrap()
}
