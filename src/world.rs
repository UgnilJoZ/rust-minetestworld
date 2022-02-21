use std::path::{Path, PathBuf};
use std::fs::File;
use std::io::{BufRead, BufReader};
use std::collections::HashMap;
use crate::MapData;
use crate::MapDataError;

#[cfg(feature = "smartstring")]
use smartstring::alias::String;

pub struct World (pub PathBuf);

impl World {
    pub fn new<P: AsRef<Path>>(path: P) -> Self {
        World(path.as_ref().to_path_buf())
    }

    pub fn get_world_metadata(&self) -> std::io::Result<HashMap<String, String>> {
        let World(path) = self;
        let file = File::open(path.with_file_name("world.mt"))?;
        let reader = BufReader::new(file);
        let mut result = HashMap::new();
        for line in reader.lines() {
            if let Some((left, right)) = line?.split_once('=') {
                result.insert(String::from(left), String::from(right));
            }
        }
        Ok(result)
    }

    pub fn get_map(&self) -> Result<MapData, WorldError> {
        let World(path) = self;
        Ok(MapData::from_sqlite_file(path.join("map.sqlite"))?)
    }
}

#[derive(thiserror::Error, Debug)]
pub enum WorldError {
    #[error("IO error: {0}")]
    IOError(#[from] std::io::Error),
    #[error("Map data error: {0}")]
    MapDataError(#[from] MapDataError),
}