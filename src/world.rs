use crate::MapData;
use crate::MapDataError;
use async_std::fs::File;
use async_std::io::BufReader;
use async_std::prelude::*;
use std::collections::HashMap;
use std::path::{Path, PathBuf};

#[cfg(feature = "smartstring")]
use smartstring::alias::String;

pub struct World(pub PathBuf);

impl World {
    pub fn new<P: AsRef<Path>>(path: P) -> Self {
        World(path.as_ref().to_path_buf())
    }

    pub async fn get_world_metadata(&self) -> std::io::Result<HashMap<String, String>> {
        let World(path) = self;
        let file = File::open(path.join("world.mt")).await?;
        let reader = BufReader::new(file);
        let mut result = HashMap::new();
        let mut lines = reader.lines();
        while let Some(line) = lines.next().await {
            if let Some((left, right)) = line?.split_once('=') {
                result.insert(String::from(left), String::from(right));
            }
        }
        Ok(result)
    }

    pub async fn get_map(&self) -> Result<MapData, WorldError> {
        let World(path) = self;
        Ok(MapData::from_sqlite_file(path.join("map.sqlite")).await?)
    }
}

#[derive(thiserror::Error, Debug)]
pub enum WorldError {
    #[error("IO error: {0}")]
    IOError(#[from] std::io::Error),
    #[error("Map data error: {0}")]
    MapDataError(#[from] MapDataError),
}
