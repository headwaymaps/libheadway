mod tileserver;

use crate::map_tiles::{Bounds, Extractor, RegionRecord, TileCollection};
use crate::{Error, ErrorContext, Result};
use axum::{
    extract::Request,
    http::StatusCode,
    middleware::{self, Next},
    response::{Html, IntoResponse, Response},
    routing::get,
    Router,
};
use pmtiles::extract::ExtractionPlan as PmtExtractionPlan;
use std::ffi::OsStr;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::RwLock;

#[derive(Clone)]
struct AppState {
    tile_collection: Arc<RwLock<TileCollection>>,
}

#[derive(uniffi::Object)]
pub struct HeadwayServer {
    extractor: Arc<RwLock<Extractor>>,
    tile_collection: Arc<RwLock<TileCollection>>,
}

/// A thin wrapper around PMTiles ExtractPlan so we can export it
#[derive(uniffi::Object)]
pub struct ExtractionPlan(pub(crate) PmtExtractionPlan);

impl From<PmtExtractionPlan> for ExtractionPlan {
    fn from(value: PmtExtractionPlan) -> Self {
        Self(value)
    }
}

#[uniffi::export]
impl ExtractionPlan {
    pub fn tile_data_length(&self) -> u64 {
        self.0.tile_data_length()
    }
}

/// A localhost tileserver backed by potentially disparate .pmtile regions.
///
/// You can add new regions in two ways:
///  1. Add an entire archive by URL: [`Self::download_system_pmtiles_if_necessary`]
///     Suitable for adding a global low resolution overview
///  2. Extract a smaller area: [`Self::prepare_pmtiles_extract`] and [`Self::extract_pmtiles_region`]
///     Suitable for adding local full resolution areas.
///
/// # Example
///
/// ```
/// use headway::{HeadwayServer, Bounds, ExtractProgress};
/// use std::sync::Arc;
///
/// struct ProgressTracker;
/// impl ExtractProgress for ProgressTracker {
///     fn on_progress(&self, progress: f64) {
///         println!("Progress: {:.1}%", progress * 100.0);
///     }
/// }
///
/// # async fn example() -> Result<(), Box<dyn std::error::Error>> {
/// let server = HeadwayServer::new(
///     "/path/to/storage",
///     "http://example.com/full-resolution-planet.pmtiles"
/// ).await?;
///
/// tokio::spawn(async move {
///     server.start("127.0.0.1:9123").await
/// });
///
/// let progress = Arc::new(ProgressTracker);
///
/// // Download a low-resolution system tileset with progress tracking
/// server.download_system_pmtiles_if_necessary(
///     "http://example.com/low-resolution-planet.pmtiles",
///     "planet-overview.pmtiles",
///     Some(progress.clone())
/// ).await?;
///
/// // Extract a specific region with progress tracking
/// let bounds = Arc::new(Bounds::nesw(47.7, -122.2, 47.5, -122.4));
///
/// let plan = server.prepare_pmtiles_extract(bounds.clone(), Some(progress.clone())).await?;
/// println!("Extract would download {} bytes of tile data", plan.tile_data_length());
///
/// server.extract_pmtiles_region(plan, Some(progress)).await?;
/// # Ok(())
/// # }
/// ```
#[uniffi::export(async_runtime = "tokio")]
impl HeadwayServer {
    /// `storage_dir`: Persists server data like pmtiles extracts
    /// `extract_source_url`: Should point to a planet file suitable for running pmtile extracts against
    #[uniffi::constructor(name = "new")]
    pub async fn new(storage_dir: &str, extract_source_url: &str) -> Result<Self> {
        let mut tiles_dir = PathBuf::from(storage_dir);
        tiles_dir.push("tiles");
        let mut tile_collection = TileCollection::new(tiles_dir);
        tile_collection
            .load_tiles_from_storage()
            .await
            .context("loading tiles from storage")?;
        let extractor = Extractor::new(extract_source_url).await?;
        Ok(Self {
            extractor: Arc::new(RwLock::new(extractor)),
            tile_collection: Arc::new(RwLock::new(tile_collection)),
        })
    }

    /// Starts the server on the given address
    pub async fn start(&self, bind_addr: &str) -> Result<()> {
        let listener = tokio::net::TcpListener::bind(bind_addr).await?;
        log::info!("Server running on http://{bind_addr}");

        let app = Router::new()
            .route("/status", get(status))
            .route(
                "/tileserver/data/default/{z}/{x}/{y_with_ext}",
                get(tileserver::get_tile),
            )
            // TODO: Handle styles/assets like a real tileserver... or maybe just use a real tileserver
            .route(
                "/tileserver/styles/basic/style.json",
                get(tileserver::get_default_style),
            )
            .route(
                "/tileserver/data/default.json",
                get(tileserver::get_tile_json),
            )
            .route(
                "/tileserver/styles/basic/sprite@2x.json",
                get(tileserver::get_sprite_json),
            )
            .route(
                "/tileserver/styles/basic/sprite@2x.png",
                get(tileserver::get_sprite_png),
            )
            .route(
                "/tileserver/fonts/{fontstack}/{range_with_ext}",
                get(tileserver::get_font),
            )
            .fallback(handler_404)
            .layer(middleware::from_fn(logging_middleware))
            .with_state(AppState {
                tile_collection: self.tile_collection.clone(),
            });

        axum::serve(listener, app).await?;
        Ok(())
    }

    /// Plans a pmtiles extraction without downloading the tile data. It does require traversing
    /// the remote index directories.
    ///
    /// Use this to determine how much data would be downloaded before committing to the extraction.
    /// Call [`Self::extract_pmtiles_region`] with the returned plan to perform the actual download.
    pub async fn prepare_pmtiles_extract(
        &self,
        bounds: Arc<Bounds>,
        progress_callback: Option<Arc<dyn crate::map_tiles::ExtractProgress>>,
    ) -> Result<ExtractionPlan> {
        let mut extractor = self.extractor.write().await;
        let plan = extractor
            .prepare_pmtiles_extract(bounds.as_ref().into(), progress_callback)
            .await?;
        Ok(plan.into())
    }

    /// Downloads the tile data for an extracted region based on the prepared plan.
    ///
    /// Call [`Self::prepare_pmtiles_extract`] first to get an [`ExtractionPlan`].
    ///
    /// Upon completion, the extracted tileset will automatically be served by the tileserver, though
    /// you may need to clear your map client's tile cache if it had previously requested the
    /// area covered by the newly added extract.
    pub async fn extract_pmtiles_region(
        &self,
        plan: Arc<ExtractionPlan>,
        progress_callback: Option<Arc<dyn crate::map_tiles::ExtractProgress>>,
    ) -> Result<RegionRecord> {
        let output_path = {
            let tile_collection = self.tile_collection.write().await;
            tile_collection.generate_user_pmtiles_path()
        };

        // extract the region to a local file
        {
            let mut extractor = self.extractor.write().await;
            extractor
                .extract_pmtiles_region(&output_path, &plan.0, progress_callback)
                .await?;
        }

        // Add the new file to the tile collection so the tileserver can serve it
        let region_record = {
            let mut collection = self.tile_collection.write().await;
            collection.add_source(&output_path).await?
        };
        log::info!(
            "Added new extracted tileset to collection: {bbox:?}",
            bbox = region_record.bounds()
        );
        Ok(region_record)
    }

    /// Delete a previously downloaded pmtiles region extract
    pub async fn remove_pmtiles_extract(&self, file_name: &str) -> Result<()> {
        let mut tile_collection = self.tile_collection.write().await;
        tile_collection.remove_extract(file_name)?;
        log::info!("Successfully removed pmtiles extract: {file_name:?}");
        Ok(())
    }

    /// Downloads a complete pmtiles file from a URL to the system tileset directory.
    ///
    /// System tilesets are permanent and cannot be deleted by users (unlike user-extracted regions).
    /// Typically used for bundling low-resolution global overview tiles.
    /// Skips download if the destination file already exists.
    pub async fn download_system_pmtiles_if_necessary(
        &self,
        source_url: &str,
        destination_filename: &str,
    ) -> Result<()> {
        let mut destination_path = {
            let tile_collection = self.tile_collection.read().await;
            tile_collection.system_root()
        };
        destination_path.push(destination_filename);
        if destination_path.extension() != Some(OsStr::new("pmtiles")) {
            return Err(Error::InvalidInput(format!(
                "destination must end with .pmtiles - got: {destination_filename}"
            )));
        }
        if std::fs::exists(&destination_path)? {
            log::debug!("{destination_filename} already exists");
            return Ok(());
        }
        log::info!("Fetching {destination_filename} from {source_url}");
        let response = reqwest::get(source_url).await?;
        let bytes = response.bytes().await?;
        std::fs::write(&destination_path, bytes)?;
        {
            let mut collection = self.tile_collection.write().await;
            collection.add_source(&destination_path).await?;
        }
        Ok(())
    }
}

async fn logging_middleware(req: Request, next: Next) -> Response {
    let method = req.method().clone();
    let uri = req.uri().clone();

    let response = next.run(req).await;
    let status = response.status();

    log::debug!("{} {} -> {}", method, uri, status);

    response
}

async fn handler_404() -> impl IntoResponse {
    (StatusCode::NOT_FOUND, "Not Found")
}

async fn status() -> Html<&'static str> {
    Html("Ok")
}
