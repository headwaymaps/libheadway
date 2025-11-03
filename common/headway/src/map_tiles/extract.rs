use crate::Result;
use pmtiles::extract::{BoundingBox, ExtractionPlan};
use pmtiles::{AsyncPmTilesReader, HashMapCache, HttpBackend};
use reqwest::Client;
use std::fs::File;
use std::io::BufWriter;
use std::path::Path;
use std::sync::Arc;

#[uniffi::export(with_foreign)]
pub trait ExtractProgress: Send + Sync {
    fn on_progress(&self, progress: f64);
}

pub struct Extractor {
    source_url: String,
    reader: Option<AsyncPmTilesReader<HttpBackend, HashMapCache>>,
}

impl Extractor {
    pub(crate) async fn new(source_url: &str) -> Result<Self> {
        Ok(Self {
            source_url: source_url.into(),
            reader: None,
        })
    }

    pub(crate) async fn reader(
        &mut self,
    ) -> Result<&mut AsyncPmTilesReader<HttpBackend, HashMapCache>> {
        if self.reader.is_none() {
            let client = Client::builder()
                .user_agent("maps.earth-ios/0.1.0")
                .build()
                .expect("nothing invalid in client builder");
            let backend = HttpBackend::try_from(client, &self.source_url)?;
            let reader =
                AsyncPmTilesReader::try_from_cached_source(backend, HashMapCache::default())
                    .await?;
            self.reader = Some(reader);
        }
        Ok(self.reader.as_mut().expect("ensured initialized just now"))
    }

    pub async fn prepare_pmtiles_extract(
        &mut self,
        bbox: BoundingBox,
        progress_callback: Option<Arc<dyn ExtractProgress>>,
    ) -> Result<ExtractionPlan> {
        log::info!("Preparing extraction");
        let callback = move |ratio| {
            if let Some(progress_callback) = &progress_callback {
                progress_callback.on_progress(ratio)
            }
        };
        let extractor = pmtiles::extract::Extractor::new(self.reader().await?).progress(&callback);
        let plan = extractor.prepare(bbox).await?;
        let size_bytes = plan.tile_data_length();
        log::info!(
            "Extract size: {} bytes ({:.2} MB)",
            size_bytes,
            size_bytes as f64 / 1_048_576.0
        );

        Ok(plan)
    }

    pub async fn extract_pmtiles_region(
        &mut self,
        output_path: &Path,
        plan: &ExtractionPlan,
        progress_callback: Option<Arc<dyn ExtractProgress>>,
    ) -> Result<()> {
        log::info!("Starting PMTiles extraction");
        log::info!("Output path: {}", output_path.display());

        let callback = move |ratio| {
            if let Some(progress_callback) = &progress_callback {
                progress_callback.on_progress(ratio)
            }
        };
        let extractor = pmtiles::extract::Extractor::new(self.reader().await?).progress(&callback);

        // Extract to a temporary file first to avoid partial files on failure
        let tmp_path = output_path.with_extension("tmp");

        let mut output_file = BufWriter::new(File::create(&tmp_path)?);

        // TODO: Pass in owned and remove this clone? Could be annoying with mobile client code.
        extractor
            .extract_to_writer(plan.clone(), &mut output_file)
            .await?;

        // Close the file before moving it
        drop(output_file);

        let size = std::fs::metadata(&tmp_path)?.len();
        std::fs::rename(&tmp_path, output_path)?;

        log::info!(
            "Successfully extracted PMTiles region to {}",
            output_path.display()
        );

        let header = extractor.input_header();
        log::info!("Extracted PMTiles info:");
        log::info!("  Tile type: {:?}", header.tile_type);
        log::info!("  Tile compression: {:?}", header.tile_compression);
        log::info!("  Extracted size: {size} bytes");

        Ok(())
    }
}
