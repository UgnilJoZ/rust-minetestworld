//! Contains the [`World`] along with [`WorldError`]

use crate::MapData;
use crate::MapDataError;
use crate::VoxelManip;
use async_std::fs::File;
use async_std::io::BufReader;
use async_std::prelude::*;
use std::collections::HashMap;
use std::path::{Path, PathBuf};

#[cfg(feature = "url")]
use url::Url;

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
    pub fn new(path: impl AsRef<Path>) -> Self {
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
            if let Some((key, value)) = line?.split_once('=') {
                result.insert(
                    String::from(key.trim_end()),
                    String::from(value.trim_start()),
                );
            }
        }
        Ok(result)
    }

    async fn get_backend_name(&self) -> Result<String, WorldError> {
        match self.get_world_metadata().await {
            Err(e) => {
                if e.kind() == std::io::ErrorKind::NotFound {
                    eprintln!("No world.mt found, falling back to sqlite3 backend");
                    Ok(String::from("sqlite3"))
                } else {
                    Err(WorldError::IOError(e))
                }
            }
            Ok(metadata) => match metadata.get("backend") {
                Some(backend) => Ok(backend.clone()),
                None => {
                    eprintln!("No backend mentioned in world.mt, falling back to sqlite3");
                    Ok(String::from("sqlite3"))
                }
            },
        }
    }

    /// Returns a handle to the map database.
    ///
    /// ```
    /// use minetestworld::World;
    /// use async_std::task;
    ///
    /// let map_data = task::block_on(async {
    ///     World::new("TestWorld").get_map_data().await.unwrap()
    /// });
    /// ```
    pub async fn get_map_data_backend(&self, read_only: bool) -> Result<MapData, WorldError> {
        let backend = self.get_backend_name().await?;
        match backend.as_str() {
            #[cfg(feature = "sqlite")]
            "sqlite3" => {
                let World(path) = self;
                Ok(MapData::from_sqlite_file(path.join("map.sqlite"), read_only).await?)
            }
            #[cfg(feature = "postgres")]
            "postgresql" => {
                let meta = self.get_world_metadata().await?;
                let connstr = meta.get("pgsql_connection").ok_or_else(|| {
                    WorldError::BogusBackendConfig(String::from(
                        "The backend 'postgres' requires a 'pgsql_connection' in world.mt",
                    ))
                })?;
                let uri = &keyvalue_to_uri_connectionstr(connstr)
                    .map_err(WorldError::BogusBackendConfig)?;
                Ok(MapData::from_pg_connection_params(uri).await?)
            }
            #[cfg(feature = "redis")]
            "redis" => {
                let meta = self.get_world_metadata().await?;
                let host = meta.get("redis_address").ok_or_else(|| {
                    WorldError::BogusBackendConfig(String::from(
                        "The backend 'redis' requires a 'redis_address' in world.mt",
                    ))
                })?;
                let host = url::Host::parse_opaque(host)?;
                let port = meta.get("redis_port").map(|p| p.parse()).transpose()?;
                let hash = meta.get("redis_hash").ok_or_else(|| {
                    WorldError::BogusBackendConfig(String::from(
                        "The backend 'redis' requires a 'redis_hash' in world.mt",
                    ))
                })?;
                Ok(MapData::from_redis_connection_params(host, port, hash).await?)
            }
            #[cfg(feature = "experimental-leveldb")]
            "leveldb" => {
                let World(path) = self;
                let path = path.clone();
                Ok(
                    task::spawn_blocking(move || MapData::from_leveldb(path.join("map.db")))
                        .await?,
                )
            }
            _ => Err(WorldError::UnknownBackend(backend)),
        }
    }

    /// Returns a handle to the map database
    ///
    /// It does not have to be explicitly closed, but may be not writable.
    ///
    /// ```
    /// use minetestworld::World;
    /// use async_std::task;
    ///
    /// let map_data = task::block_on(async {
    ///     World::new("TestWorld").get_map_data().await.unwrap()
    /// });
    /// ```
    pub async fn get_map_data(&self) -> Result<MapData, WorldError> {
        self.get_map_data_backend(true).await
    }

    /// Returns a writable handle to the map database
    ///
    /// It has to be explicitly closed, since the sqlite3 dirty flag may be set.
    ///
    /// ```ignore
    /// use minetestworld::World;
    /// use async_std::task;
    ///
    /// let map_data = task::block_on(async {
    ///     World::new("TestWorld").get_mutable_map_data().await.unwrap()
    /// });
    /// ```
    pub async fn get_mutable_map_data(&self) -> Result<MapData, WorldError> {
        self.get_map_data_backend(true).await
    }

    /// Returns a VoxelManip with the ability to read and write nodes
    pub async fn get_voxel_manip(&self, writable: bool) -> Result<VoxelManip, WorldError> {
        Ok(VoxelManip::new(self.get_map_data_backend(!writable).await?))
    }
}

/// Represents a failure to interact with the world
#[derive(thiserror::Error, Debug)]
pub enum WorldError {
    #[error("IO error: {0}")]
    /// An IO error happened
    IOError(#[from] std::io::Error),
    #[error("Map data error: {0}")]
    /// The map data backend returned an error
    MapDataError(#[from] MapDataError),
    #[error("Unknown backend '{0}'")]
    /// The map data backend is not known or implemented
    UnknownBackend(String),
    #[error("Bogus backend config: {0}")]
    /// The map data backend config contains an error
    ///
    /// A description is included.
    BogusBackendConfig(String),
    #[error("Host parse error: {0}")]
    #[cfg(feature = "redis")]
    /// Failure to parse an URL
    ParseUrlError(#[from] url::ParseError),
    #[error("Parse int error: {0}")]
    /// Failure to parse an int from a string
    ParseIntError(#[from] std::num::ParseIntError),
}

/// Converts a postgres connection string from keyvalue to URI
#[cfg(feature = "postgres")]
fn keyvalue_to_uri_connectionstr(
    keyword_value: &str,
) -> Result<std::string::String, std::string::String> {
    let mut params: HashMap<&str, &str> = keyword_value
        .split_whitespace()
        .filter_map(|s| s.split_once('='))
        .collect();

    let mut url = Url::parse("postgresql://").unwrap();
    let host = params.remove("host").unwrap_or("localhost");
    url.set_host(Some(host)).map_err(|e| format!("{e}"))?;
    let port = params
        .remove("host")
        .map(|s| s.parse::<u16>())
        .ok_or_else(|| String::from("port is not a valid number"))?
        .unwrap_or(5432);
    url.set_port(Some(port))
        .map_err(|_| std::string::String::new())?;

    if let Some(user) = params.remove("user") {
        url.set_username(user)
            .map_err(|_| std::string::String::new())?;
    }
    url.set_password(params.remove("password"))
        .map_err(|_| String::new())?;

    url.set_path(params.remove("dbname").unwrap_or_default());

    for (key, value) in params {
        url.query_pairs_mut().append_pair(key, value);
    }

    Ok(url.into())
}
