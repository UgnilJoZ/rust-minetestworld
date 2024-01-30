//! Contains a type to read a world's map data
#[cfg(feature = "experimental-leveldb")]
use async_std::sync::{Arc, Mutex};
use futures::future;
use futures::stream;
use futures::stream::BoxStream;
use futures::stream::StreamExt;
use futures::TryStreamExt;
#[cfg(feature = "experimental-leveldb")]
use leveldb_rs::{LevelDBError, DB as LevelDb};
use log::LevelFilter;
#[cfg(feature = "redis")]
use redis::{aio::MultiplexedConnection as RedisConn, AsyncCommands};
#[cfg(feature = "sqlite")]
use sqlx::sqlite::{SqliteConnectOptions, SqlitePool};
#[cfg(feature = "postgres")]
use sqlx::{postgres::PgConnectOptions, PgPool};
#[cfg(any(feature = "sqlite", feature = "postgres"))]
use sqlx::{prelude::*, ConnectOptions};
#[cfg(any(feature = "sqlite", feature = "experimental-leveldb"))]
use std::path::Path;
use std::str::FromStr;
#[cfg(feature = "redis")]
use url::Host;

use crate::map_block::{MapBlock, MapBlockError};
use crate::positions::Position;

const POSTGRES_QUERY: &str = "SELECT data FROM blocks
 WHERE (posx = $1 AND posy = $2 AND posz = $3)";

const SQLITE_UPSERT: &str = "INSERT INTO blocks VALUES (?, ?)
 ON CONFLICT(pos) DO UPDATE SET data=excluded.data";

const POSTGRES_UPSERT: &str = "INSERT INTO blocks VALUES($1, $2, $3, $4)
 ON CONFLICT(posx,posy,posz) DO UPDATE SET data=excluded.data";

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
    #[error("MapBlock {0:?} does not exist")]
    MapBlockNonexistent(Position),

    /// An IO related error
    #[error("IO error: {0}")]
    IoError(#[from] std::io::Error),
}

impl MapDataError {
    /// Converts an SQL error to a mapblock error
    ///
    /// while converting `RowNotFound` to `MapBlockNonexistent(pos)`
    #[cfg(any(feature = "sqlite", feature = "postgres"))]
    fn from_sqlx_error(e: sqlx::Error, pos: Position) -> MapDataError {
        if let sqlx::Error::RowNotFound = e {
            MapDataError::MapBlockNonexistent(pos)
        } else {
            MapDataError::SqlError(e)
        }
    }
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
        /// The Hash in which the world's data is stored in
        hash: std::string::String,
    },

    /// This variant is a thread-safe open LevelDB
    #[cfg(feature = "experimental-leveldb")]
    LevelDb(Arc<Mutex<LevelDb>>),
}

impl MapData {
    #[cfg(feature = "sqlite")]
    /// Connects to the "map.sqlite" database.
    ///
    /// If the `blocks` table does not exist, tries to create it.
    ///
    /// ```
    /// use minetestworld::MapData;
    /// use async_std::task;
    ///
    /// let meta = task::block_on(async {
    ///     MapData::from_sqlite_file("TestWorld/map.sqlite", false).await.unwrap();
    /// });
    /// ```
    pub async fn from_sqlite_file(
        filename: impl AsRef<Path>,
        read_only: bool,
    ) -> Result<MapData, MapDataError> {
        let opts = SqliteConnectOptions::new()
            .immutable(read_only)
            .filename(filename)
            .create_if_missing(!read_only)
            .log_statements(LevelFilter::Debug);
        match SqlitePool::connect_with(opts).await {
            Ok(pool) => {
                sqlx::query("CREATE TABLE IF NOT EXISTS blocks (`pos` INT NOT NULL PRIMARY KEY,`data` BLOB)").execute(&pool).await?;
                Ok(MapData::Sqlite(pool))
            }
            Err(e) => Err(MapDataError::SqlError(e)),
        }
    }

    #[cfg(feature = "postgres")]
    /// Connects to a Postgres database
    pub async fn from_pg_connection_params(url: &str) -> Result<MapData, MapDataError> {
        let opts = PgConnectOptions::from_str(url)?.log_statements(LevelFilter::Debug);
        Ok(MapData::Postgres(PgPool::connect_with(opts).await?))
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
            hash: hash.to_string(),
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
    pub async fn all_mapblock_positions(&self) -> BoxStream<Result<Position, MapDataError>> {
        match self {
            #[cfg(feature = "sqlite")]
            MapData::Sqlite(pool) => sqlx::query_as("SELECT pos FROM blocks")
                .fetch(pool)
                .map_err(MapDataError::SqlError)
                .boxed(),
            #[cfg(feature = "postgres")]
            MapData::Postgres(pool) => sqlx::query_as("SELECT posx, posy, posz FROM blocks")
                .fetch(pool)
                .map_err(MapDataError::SqlError)
                .boxed(),
            #[cfg(feature = "redis")]
            MapData::Redis { connection, hash } => {
                // We can't really stream, so we'll just collect the result with hkeys
                let positions: Result<Vec<i64>, _> =
                    connection.clone().hkeys(hash.to_string()).await;
                match positions {
                    Ok(positions) => stream::iter(
                        positions
                            .into_iter()
                            .map(Position::from_database_key)
                            .map(Ok),
                    )
                    .boxed(),
                    Err(e) => stream::once(future::ready(Err(MapDataError::RedisError(e)))).boxed(),
                }
            }
            #[cfg(feature = "experimental-leveldb")]
            MapData::LevelDb(db) =>
            // TODO Use task::spawn_blocking for this, as this blocks the thread for a longer time
            {
                stream::iter(
                    db.lock()
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
                        .map(get_integer_as_block),
                )
                .boxed()
            }
        }
    }

    /// Queries the backend for the data of a single mapblock
    pub async fn get_block_data(&self, pos: Position) -> Result<Vec<u8>, MapDataError> {
        let pos_index = pos.as_database_key();
        match self {
            #[cfg(feature = "sqlite")]
            MapData::Sqlite(pool) => sqlx::query("SELECT data FROM blocks WHERE pos = ?")
                .bind(pos_index)
                .fetch_one(pool)
                .await
                .and_then(|row| row.try_get("data"))
                .map_err(|e| MapDataError::from_sqlx_error(e, pos)),
            #[cfg(feature = "postgres")]
            MapData::Postgres(pool) => sqlx::query(POSTGRES_QUERY)
                .bind(pos.x)
                .bind(pos.y)
                .bind(pos.z)
                .fetch_one(pool)
                .await
                .and_then(|row| row.try_get("data"))
                .map_err(|e| MapDataError::from_sqlx_error(e, pos)),
            #[cfg(feature = "redis")]
            MapData::Redis { connection, hash } => {
                let value: Option<_> = connection.clone().hget(hash.to_string(), pos_index).await?;
                value.ok_or(MapDataError::MapBlockNonexistent(pos))
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
    /// `pos` is a map block position; this means that every dimension is divided
    /// by the side length of a map block.
    pub async fn get_mapblock(&self, pos: Position) -> Result<MapBlock, MapDataError> {
        Ok(MapBlock::from_data(
            self.get_block_data(pos).await?.as_slice(),
        )?)
    }

    /// Sets the backend's mapblock data for position `pos` to `data`
    pub async fn set_mapblock_data(&self, pos: Position, data: &[u8]) -> Result<(), MapDataError> {
        match self {
            #[cfg(feature = "sqlite")]
            MapData::Sqlite(pool) => sqlx::query(SQLITE_UPSERT)
                .bind(pos.as_database_key())
                .bind(data)
                .execute(pool)
                .await
                .map(|_| {})
                .map_err(MapDataError::SqlError),
            #[cfg(feature = "postgres")]
            MapData::Postgres(pool) => sqlx::query(POSTGRES_UPSERT)
                .bind(pos.x)
                .bind(pos.y)
                .bind(pos.z)
                .bind(data)
                .execute(pool)
                .await
                .map(|_| {})
                .map_err(MapDataError::SqlError),
            #[cfg(feature = "redis")]
            MapData::Redis { connection, hash } => connection
                .clone()
                .hset(hash, pos.as_database_key(), data)
                .await
                .map_err(|e| e.into()),
        }
    }

    /// Inserts or replaces the map block at `pos`
    pub async fn set_mapblock(&self, pos: Position, block: &MapBlock) -> Result<(), MapDataError> {
        self.set_mapblock_data(pos, &block.to_binary()?).await
    }
}
