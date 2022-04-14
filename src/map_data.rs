//! Contains a type to read a world's map data
#[cfg(feature = "experimental-leveldb")]
use async_std::sync::{Arc, Mutex};
#[cfg(any(feature = "sqlite", feature = "postgres"))]
use futures::stream::TryStreamExt;
#[cfg(feature = "experimental-leveldb")]
use leveldb_rs::{LevelDBError, DB as LevelDb};
#[cfg(feature = "redis")]
use redis::{aio::MultiplexedConnection as RedisConn, AsyncCommands};
#[cfg(feature = "smartstring")]
use smartstring::alias::String;
#[cfg(any(feature = "sqlite", feature = "postgres"))]
use sqlx::prelude::*;
#[cfg(feature = "sqlite")]
use sqlx::sqlite::{SqliteConnectOptions, SqlitePool};
#[cfg(feature = "postgres")]
use sqlx::PgPool;
#[cfg(any(feature = "sqlite", feature = "experimental-leveldb"))]
use std::path::Path;
#[cfg(feature = "redis")]
use url::Host;

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

    #[cfg(feature = "experimental-leveldb")]
    #[error("LevelDB error: {0}")]
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
    /// This variant covers the SQLite database backend
    #[cfg(feature = "sqlite")]
    Sqlite(SqlitePool),

    /// This variant supports PostgreSQL as a backend
    #[cfg(feature = "postgres")]
    Postgres(PgPool),

    /// This variant supports Redis as database backend
    #[cfg(feature = "redis")]
    Redis {
        /// The connection to the Redis instance
        connection: RedisConn,
        /// The hash in which the world's data is stored in
        hash: String,
    },

    /// This variant is a thread-safe open LevelDB
    #[cfg(feature = "experimental-leveldb")]
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

    #[cfg(feature = "postgres")]
    /// Connects to a Postgres database
    pub async fn from_pg_connection_params(url: &str) -> Result<MapData, MapDataError> {
        Ok(MapData::Postgres(PgPool::connect(url).await?))
    }

    #[cfg(feature = "redis")]
    /// Connects to a Redis server given the connection parameters
    pub async fn from_redis_connection_params(
        host: Host,
        port: Option<u16>,
        hash: &str,
    ) -> Result<MapData, MapDataError> {
        Ok(MapData::Redis {
            connection: redis::Client::open(format!(
                "redis://{host}{}/",
                port.map(|p| format!(":{p}")).unwrap_or_default()
            ))?
            .get_multiplexed_async_std_connection()
            .await?,
            hash: String::from(hash),
        })
    }

    #[cfg(feature = "experimental-leveldb")]
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
            #[cfg(feature = "postgres")]
            MapData::Postgres(pool) => {
                let mut result = vec![];
                let mut rows = sqlx::query("SELECT posx, posy, posz FROM blocks")
                    .bind("x")
                    .bind("y")
                    .bind("z")
                    .fetch(pool);
                while let Some(row) = rows.try_next().await? {
                    let x: i32 = row.try_get("posx")?;
                    let y: i32 = row.try_get("posy")?;
                    let z: i32 = row.try_get("posz")?;
                    let pos = Position {
                        x: x as i16,
                        y: y as i16,
                        z: z as i16,
                    };
                    result.push(pos);
                }
                Ok(result)
            }
            #[cfg(feature = "redis")]
            MapData::Redis { connection, hash } => {
                let v: Vec<i64> = connection.clone().hkeys(hash.to_string()).await?;
                Ok(v.into_iter().map(get_integer_as_block).collect())
            }
            #[cfg(feature = "experimental-leveldb")]
            MapData::LevelDb(db) =>
            // TODO Use task::spawn_blocking for this, as this blocks the thread for a longer time
            {
                Ok(db
                    .lock()
                    .await
                    .iter()
                    .map_err(MapDataError::LevelDbError)?
                    .alloc()
                    //.inspect(|(key, _value)| println!("{key:?}"))
                    // Now here it gets interesting. Figure out why the key's length is often 9 bytes instead of 8 bytes.
                    .filter(|(key, _)| key.len() == 8)
                    // And figure out why LevelDB reports corrupted blocks
                    .map(|(key, _value)| Ok(i64::from_le_bytes(key.try_into()?)))
                    .filter_map(|key: Result<i64, Vec<u8>>| key.ok())
                    .map(get_integer_as_block)
                    .collect())
            }
        }
    }

    /// Queries the backend for the data of a single mapblock
    pub async fn get_block_data(&self, pos: Position) -> Result<Vec<u8>, MapDataError> {
        let pos_index = get_block_as_integer(pos);
        match self {
            #[cfg(feature = "sqlite")]
            MapData::Sqlite(pool) => Ok(sqlx::query("SELECT data FROM blocks WHERE pos = ?")
                .bind(pos_index)
                .fetch_one(pool)
                .await?
                .try_get("data")?),
            #[cfg(feature = "postgres")]
            MapData::Postgres(pool) => Ok(sqlx::query(
                "SELECT data FROM blocks WHERE (posx = $1 AND posy = $2 AND posz = $3)",
            )
            .bind(pos.x)
            .bind(pos.y)
            .bind(pos.z)
            .fetch_one(pool)
            .await?
            .try_get("data")?),
            #[cfg(feature = "redis")]
            MapData::Redis { connection, hash } => {
                Ok(connection.clone().hget(hash.to_string(), pos_index).await?)
            }
            #[cfg(feature = "experimental-leveldb")]
            MapData::LevelDb(db) => Ok(db
                .lock()
                .await
                .get(&pos_index.to_le_bytes())
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
