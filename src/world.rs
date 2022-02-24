use crate::MapData;
use crate::MapDataError;
use async_std::fs::File;
use async_std::io::BufReader;
use async_std::prelude::*;
use std::collections::HashMap;
use std::path::{Path, PathBuf};

#[cfg(feature = "smartstring")]
use smartstring::alias::String;

/// A Minetest world
/// 
/// ```
/// use minetestworld::World;
/// 
/// let world = World::new("TestWorld");
/// ```
pub struct World(pub PathBuf);

impl World {
    /// Creates a new world object from a directory path.
    /// 
    /// No further checks are done, e.g. for existence of essential files.
    pub fn new<P: AsRef<Path>>(path: P) -> Self {
        World(path.as_ref().to_path_buf())
    }

    /// Reads the basic metadata of the world.
    ///
    /// ```
    /// use minetestworld::World;
    /// use async_std::task;
    /// 
    /// let meta = task::block_on(async {
    ///     World::new("TestWorld").get_world_metadata().await
    /// }).unwrap();
    /// assert_eq!(meta.get("world_name").unwrap(), "Hallo");
    /// assert_eq!(meta.get("backend").unwrap(), "sqlite3");
    /// assert_eq!(meta.get("gameid").unwrap(), "minetest");
    /// ```
    pub async fn get_world_metadata(&self) -> std::io::Result<HashMap<String, String>> {
        let World(path) = self;
        let file = File::open(path.join("world.mt")).await?;
        let reader = BufReader::new(file);
        let mut result = HashMap::new();
        let mut lines = reader.lines();
        while let Some(line) = lines.next().await {
            if let Some((left, right)) = line?.split_once('=') {
                result.insert(String::from(left.trim_end()), String::from(right.trim_start()));
            }
        }
        Ok(result)
    }

    async fn get_backend(&self) -> Result<String, WorldError> {
        match self.get_world_metadata().await {
            Err(e) => if e.kind() == std::io::ErrorKind::NotFound {
                eprintln!("No world.mt found, falling back to sqlite3 backend");
                Ok(String::from("sqlite3"))
            } else {
                Err(WorldError::IOError(e))
            },
            Ok(metadata) => match metadata.get("backend") {
                Some(backend) => Ok(backend.clone()),
                None => {
                    eprintln!("No backend mentioned in world.mt, falling back to sqlite3");
                    Ok(String::from("sqlite3"))
                }
            },
        }
    }

    /// Reads the basic metadata of the world.
    ///
    /// ```
    /// use minetestworld::World;
    /// use async_std::task;
    /// 
    /// let meta = task::block_on(async {
    ///     World::new("TestWorld").get_map().await.unwrap()
    /// });
    /// ```
    pub async fn get_map(&self) -> Result<MapData, WorldError> {
        let backend = self.get_backend().await?;
        if backend == "sqlite3" {
            let World(path) = self;
            Ok(MapData::from_sqlite_file(path.join("map.sqlite")).await?)
        } else {
            Err(WorldError::UnknownBackend(backend))
        }
    }
}

#[derive(thiserror::Error, Debug)]
pub enum WorldError {
    #[error("IO error: {0}")]
    IOError(#[from] std::io::Error),
    #[error("Map data error: {0}")]
    MapDataError(#[from] MapDataError),
    #[error("Unknown backend '{0}'")]
    UnknownBackend(String)
}
