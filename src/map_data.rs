//! Contains a type to read a world's map data
use async_std::sync::{Arc, Mutex};
use futures::stream::TryStreamExt;
#[cfg(feature = "leveldb")]
use leveldb_rs::{LevelDBError, DB as LevelDb};
#[cfg(feature = "redis")]
use redis::{aio::MultiplexedConnection as RedisConn, AsyncCommands};
use sqlx::prelude::*;
#[cfg(any(feature = "sqlite", feature = "postgres"))]
use sqlx::sqlite::{SqliteConnectOptions, SqlitePool};
use std::path::Path;
use url::Host;

#[cfg(feature = "smartstring")]
use smartstring::alias::String;

use crate::map_block::{MapBlock, MapBlockError, Node, NodeIter};
use crate::positions::{get_block_as_integer, get_integer_as_block, Position};

/// An error in the underlying database or in the map block binary format
#[derive(thiserror::Error, Debug)]
pub enum MapDataError {
    #[cfg(any(feature = "sqlite", feature = "postgres"))]
    #[error("Database error: {0}")]
    /// sqlx based error. This covers Sqlite and Postgres errors.
    SqlError(#[from] sqlx::Error),
    #[cfg(feature = "redis")]
    #[error("Database error: {0}")]
    /// Redis connection error
    RedisError(#[from] redis::RedisError),
    #[cfg(feature = "leveldb")]
    #[error("Database error: {0}")]
    /// LevelDB error
    LevelDbError(LevelDBError),
    #[error("MapBlockError: {0}")]
    /// Error while reading a map block
    MapBlockError(#[from] MapBlockError),
    /// This mapblock does not exist
    #[error("MapBlock {0} does not exist")]
    MapBlockNonexistent(i64),
}

/// A handle to the world data
///
/// Can be used to query MapBlocks and nodes.
pub enum MapData {
    #[cfg(any(feature = "sqlite", feature = "postgres"))]
    /// This variant covers the SQLite and PostgreSQL database backends
    Sqlite(SqlitePool),
    #[cfg(feature = "redis")]
    /// This variant supports Redis as database backend
    Redis {
        /// The connection to the Redis instance
        connection: RedisConn,
        /// The hash in which the world's data is stored in
        hash: String,
    },
    #[cfg(feature = "leveldb")]
    /// This variant is a thread-safe open LevelDB
    LevelDb(Arc<Mutex<LevelDb>>),
}

impl MapData {
    #[cfg(feature = "sqlite")]
    /// Connects to the "map.sqlite" database.
    ///
    /// ```
    /// use minetestworld::MapData;
    /// use async_std::task;
    ///
    /// let meta = task::block_on(async {
    ///     MapData::from_sqlite_file("TestWorld/map.sqlite").await.unwrap();
    /// });
    /// ```
    pub async fn from_sqlite_file(filename: impl AsRef<Path>) -> Result<MapData, MapDataError> {
        Ok(MapData::Sqlite(
            SqlitePool::connect_with(
                SqliteConnectOptions::new()
                    .immutable(true)
                    .filename(filename),
            )
            .await?,
        ))
    }

    #[cfg(feature = "redis")]
    /// Connects to a Redis server given the connection parameters
    pub async fn from_redis_connection_params(
        host: Host,
        port: Option<u16>,
        hash: String,
    ) -> Result<MapData, MapDataError> {
        Ok(MapData::Redis {
            connection: redis::Client::open(format!(
                "redis://{host}{}/",
                port.map(|p| format!(":{p}")).unwrap_or_default()
            ))?
            .get_multiplexed_async_std_connection()
            .await?,
            hash,
        })
    }

    #[cfg(feature = "leveldb")]
    /// Opens a local LevelDB database
    pub fn from_leveldb(leveldb_directory: impl AsRef<Path>) -> Result<MapData, MapDataError> {
        let db = LevelDb::open(leveldb_directory.as_ref()).map_err(MapDataError::LevelDbError)?;
        Ok(MapData::LevelDb(Arc::new(Mutex::new(db))))
    }

    /// Returns the positions of all mapblocks
    ///
    /// Note that the unit of the coordinates will be
    /// [MAPBLOCK_LENGTH][`crate::map_block::MAPBLOCK_LENGTH`].
    pub async fn all_mapblock_positions(&self) -> Result<Vec<Position>, MapDataError> {
        match self {
            #[cfg(feature = "sqlite")]
            MapData::Sqlite(pool) => {
                let mut result = vec![];
                let mut rows = sqlx::query("SELECT pos FROM blocks")
                    .bind("pos")
                    .fetch(pool);
                while let Some(row) = rows.try_next().await? {
                    let pos_index = row.try_get("pos")?;
                    result.push(get_integer_as_block(pos_index));
                }
                Ok(result)
            }
            #[cfg(feature = "redis")]
            MapData::Redis { connection, hash } => {
                let mut v: Vec<i64> = connection.clone().hkeys(hash.to_string()).await?;
                Ok(v.drain(..).map(get_integer_as_block).collect())
            }
            #[cfg(feature = "leveldb")]
            MapData::LevelDb(db) => {
                let mut db = db.lock().await.clone();
                async_std::task::spawn_blocking(move || {
                    Ok(db
                        .iter()?
                        .alloc()
                        .map(|(key, _value)| Ok(i64::from_le_bytes(key.try_into()?)))
                        .filter_map(|key: Result<i64, Vec<u8>>| key.ok())
                        .map(get_integer_as_block)
                        .collect())
                })
                .await
                .map_err(MapDataError::LevelDbError)
            }
        }
    }

    /// Queries the backend for the data of a single mapblock
    pub async fn get_block_data(&self, pos: Position) -> Result<Vec<u8>, MapDataError> {
        let pos = get_block_as_integer(pos);
        match self {
            #[cfg(feature = "sqlite")]
            MapData::Sqlite(pool) => Ok(sqlx::query("SELECT data FROM blocks WHERE pos = ?")
                .bind(pos)
                .fetch_one(pool)
                .await?
                .try_get("data")?),
            #[cfg(feature = "redis")]
            MapData::Redis { connection, hash } => {
                Ok(connection.clone().hget(hash.to_string(), pos).await?)
            }
            #[cfg(feature = "leveldb")]
            MapData::LevelDb(db) => Ok(db
                .lock()
                .await
                .get(&pos.to_le_bytes())
                .map_err(MapDataError::LevelDbError)?
                .ok_or(MapDataError::MapBlockNonexistent(pos))?),
        }
    }

    /// Queries the backend for a specific map block
    ///
    /// `pos` is a map block position.
    pub async fn get_mapblock(&self, pos: Position) -> Result<MapBlock, MapDataError> {
        Ok(MapBlock::from_data(
            self.get_block_data(pos).await?.as_slice(),
        )?)
    }

    /// Enumerate all nodes from the mapblock at `pos`
    ///
    /// Returns all nodes along with their relative position within the map block
    pub async fn iter_mapblock_nodes(
        &self,
        mapblock_pos: Position,
    ) -> Result<impl Iterator<Item = (Position, Node)>, MapDataError> {
        let data = self.get_block_data(mapblock_pos).await?;
        let mapblock = MapBlock::from_data(data.as_slice())?;
        Ok(NodeIter::new(mapblock, mapblock_pos))
    }
}
