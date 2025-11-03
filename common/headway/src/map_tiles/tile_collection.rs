// TODO:
// - move the pmtiles collection stuff here.
//      - initialize from directory
// - Have the webserver state reference this new entity
// - have this entity call the extract logic to mutate its own state (so we don't need to restart service)

use super::{Bounds, RegionRecord};
use crate::{Error, ErrorContext, Result};
use bytes::Bytes;
use pmtiles::{AsyncPmTilesReader, MmapBackend, TileCoord};
use std::fmt::Formatter;
use std::fs;
use std::path::{Path, PathBuf};
use uuid::Uuid;

struct PmTilesSource {
    reader: AsyncPmTilesReader<MmapBackend>,
    record: RegionRecord,
    path: PathBuf,
}

impl std::fmt::Debug for PmTilesSource {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PMTilesSource")
            .field("file_name", &self.record.file_name)
            .field("file_size", &self.record.file_size)
            .finish()
    }
}

impl PmTilesSource {
    async fn get_tile(&self, z: u8, x: u32, y: u32) -> Result<Option<Bytes>> {
        let tile_coord = TileCoord::new(z, x, y)?;
        Ok(self.reader.get_tile(tile_coord).await?)
    }
}

#[derive(Debug)]
pub struct TileCollection {
    pmtiles_sources: Vec<PmTilesSource>,
    pub(crate) file_root: PathBuf,
}

impl TileCollection {
    pub fn new(file_root: PathBuf) -> Self {
        Self {
            pmtiles_sources: vec![],
            file_root,
        }
    }

    pub(crate) fn user_extracts_root(&self) -> PathBuf {
        let mut path = self.file_root.clone();
        path.push("user");
        path
    }

    pub(crate) fn system_root(&self) -> PathBuf {
        let mut path = self.file_root.clone();
        path.push("system");
        path
    }

    pub(crate) fn generate_user_pmtiles_path(&self) -> PathBuf {
        let mut path = self.user_extracts_root();
        path.push(Uuid::new_v4().to_string());
        path.with_extension("pmtiles")
    }

    pub fn remove_extract(&mut self, file_name: &str) -> Result<()> {
        let Some(pos) = self
            .pmtiles_sources
            .iter()
            .position(|x| x.record.file_name == file_name)
        else {
            return Err(Error::Runtime(format!(
                "no pmtiles source exists with file_name: {file_name}"
            )));
        };
        let path = &self.pmtiles_sources[pos].path;
        assert!(fs::exists(path)?);
        if !is_path_within_dir(path, &self.user_extracts_root())? {
            return Err(Error::Runtime(format!(
                "Can only remove extracts within user tile dir: {path:?}"
            )));
        }
        fs::remove_file(path)?;
        self.pmtiles_sources.remove(pos);
        Ok(())
    }

    pub(crate) async fn load_tiles_from_storage(&mut self) -> Result<()> {
        fs::create_dir_all(self.system_root())?;
        fs::create_dir_all(self.user_extracts_root())?;

        // Scan directory for .pmtiles files
        for entry in
            fs::read_dir(self.system_root())?.chain(fs::read_dir(self.user_extracts_root())?)
        {
            let path = entry?.path();

            // Only process .pmtiles files
            if path.extension().and_then(|s| s.to_str()) != Some("pmtiles") {
                continue;
            }
            match self.add_source(&path).await {
                Ok(_) => {}
                Err(e) => {
                    log::error!("Skipping pmtiles source: {path:?} due to error: {e}")
                }
            }
        }

        if self.pmtiles_sources.is_empty() {
            log::warn!("No PMTiles files found in directory: {:?}", self.file_root);
        } else {
            log::info!("Loaded {} PMTiles source(s)", self.pmtiles_sources.len());
        }

        Ok(())
    }

    pub(crate) async fn get_tile(&self, z: u8, x: u32, y: u32) -> Result<Option<Bytes>> {
        for source in &self.pmtiles_sources {
            if let Some(tile) = source.get_tile(z, x, y).await? {
                log::debug!(
                    "Found tile {z}/{x}/{y} in source: {:?}",
                    source.path.file_name().expect("filename must be set")
                );
                return Ok(Some(tile));
            }
        }
        Ok(None)
    }

    pub async fn add_source(&mut self, path: &Path) -> Result<RegionRecord> {
        let Some(path_display) = path.file_name().and_then(|name| {
            path.parent()
                .and_then(|p| p.file_name())
                .map(|parent| Path::new(parent).join(name))
        }) else {
            return Err(Error::Runtime(format!("invalid source path: {path:?}")));
        };
        log::debug!("Adding PMTiles file: {}", path_display.display());
        let reader = AsyncPmTilesReader::new_with_path(&path)
            .await
            .context(format!("pmtiles archive: {path:?}"))?;

        let header = reader.get_header();
        let bounds = Bounds {
            min_lon: header.min_longitude,
            min_lat: header.min_latitude,
            max_lon: header.max_longitude,
            max_lat: header.max_latitude,
        };

        log::info!(
            "  Loaded {} - bbox: {bounds:?}, zoom: {}-{}",
            path_display.display(),
            header.min_zoom,
            header.max_zoom
        );

        let file_name = path
            .file_name()
            .expect("file name must be present")
            .to_str()
            .expect("names are valid by construction")
            .to_string();
        let file_size = fs::metadata(path)?.len();
        let pmt_record = RegionRecord {
            file_name,
            file_size,
            bounds,
        };

        self.pmtiles_sources.push(PmTilesSource {
            reader,
            path: path.to_path_buf(),
            record: pmt_record.clone(),
        });
        Ok(pmt_record)
    }
}

fn is_path_within_dir(path: &Path, dir: &Path) -> std::io::Result<bool> {
    let path = path.canonicalize()?;
    let dir = dir.canonicalize()?;
    Ok(path.starts_with(dir))
}
