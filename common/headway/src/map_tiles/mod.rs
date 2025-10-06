mod tile_collection;

pub(crate) use tile_collection::TileCollection;

mod extract;
pub(crate) use extract::{ExtractProgress, Extractor};

#[derive(Clone, Debug, uniffi::Object)]
pub struct Bounds {
    max_lat: f64,
    max_lon: f64,
    min_lat: f64,
    min_lon: f64,
}

#[uniffi::export]
impl Bounds {
    #[uniffi::constructor]
    pub fn nesw(max_lat: f64, max_lon: f64, min_lat: f64, min_lon: f64) -> Self {
        Self {
            max_lat,
            max_lon,
            min_lat,
            min_lon,
        }
    }
}

impl From<&Bounds> for pmtiles::extract::BoundingBox {
    fn from(value: &Bounds) -> Self {
        Self {
            min_lon: value.min_lon,
            min_lat: value.min_lat,
            max_lon: value.max_lon,
            max_lat: value.max_lat,
        }
    }
}

#[derive(Debug, Clone, uniffi::Object)]
pub struct RegionRecord {
    bounds: Bounds,
    file_name: String,
    file_size: u64,
}

#[uniffi::export]
impl RegionRecord {
    pub fn bounds(&self) -> Bounds {
        self.bounds.clone()
    }
    pub fn file_name(&self) -> String {
        self.file_name.to_string()
    }
    pub fn file_size(&self) -> u64 {
        self.file_size
    }
}
