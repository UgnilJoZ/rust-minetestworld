//! Contains a type to read a world's map data
use futures::future;
use futures::stream;
use futures::stream::BoxStream;
use futures::stream::StreamExt;
use futures::TryStreamExt;
use log::LevelFilter;
#[cfg(feature = "redis")]
use redis::{aio::MultiplexedConnection as RedisConn, AsyncCommands};
#[cfg(feature = "leveldb")]
use rusty_leveldb::{LdbIterator, Options as LdbOptions, Status as LdbStatus, DB as LdbCon};
#[cfg(feature = "sqlite")]
use sqlx::query::Query;
#[cfg(feature = "sqlite")]
use sqlx::sqlite::SqliteArguments;
#[cfg(feature = "sqlite")]
use sqlx::sqlite::{SqliteConnectOptions, SqlitePool, SqliteRow};
#[cfg(feature = "sqlite")]
use sqlx::Sqlite;
#[cfg(feature = "postgres")]
use sqlx::{postgres::PgConnectOptions, PgPool};
#[cfg(any(feature = "sqlite", feature = "postgres"))]
use sqlx::{prelude::*, ConnectOptions};
use std::num::ParseIntError;
#[cfg(any(feature = "sqlite", feature = "leveldb"))]
use std::path::Path;
use std::str::FromStr;
#[cfg(feature = "leveldb")]
use std::sync::Arc;
#[cfg(feature = "leveldb")]
use tokio::sync::Mutex;
#[cfg(feature = "redis")]
use url::Host;

use crate::map_block::{MapBlock, MapBlockError};
use crate::positions::Position;

const POSTGRES_QUERY: &str = "SELECT data FROM blocks
 WHERE (posx = $1 AND posy = $2 AND posz = $3)";

const SQLITE_UPSERT_UNIFIED_POS: &str = "INSERT INTO blocks VALUES (?, ?)
 ON CONFLICT(pos) DO UPDATE SET data=excluded.data";

const SQLITE_UPSERT: &str = "INSERT INTO blocks(x, y, z, data) VALUES (?, ?, ?, ?)
 ON CONFLICT(x, y, z) DO UPDATE SET data=excluded.data";

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

    #[cfg(feature = "leveldb")]
    #[error("LevelDB error: {0}")]
    /// LevelDB error
    LevelDbError(#[from] LdbStatus),

    #[error("MapBlockError: {0}")]
    /// Error while reading a map block
    MapBlockError(#[from] MapBlockError),

    /// This mapblock does not exist
    #[error("MapBlock {0:?} does not exist")]
    MapBlockNonexistent(Position),

    /// An IO related error
    #[error("IO error: {0}")]
    IoError(#[from] std::io::Error),

    /// An error while parsing a string as integer value
    #[error("Error parsing string as integer value: {0}")]
    ParseIntError(#[from] ParseIntError),
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
    Sqlite {
        /// The handle to the database client
        connection: SqlitePool,
        /// True if the opened database uses the [current position schema](https://github.com/luanti-org/luanti/pull/15768).
        ///
        /// If False, the block position is stored in a single integer column.
        split_blockpos: bool,
    },

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
    #[cfg(feature = "leveldb")]
    LevelDb(Arc<Mutex<LdbCon>>),
}

impl MapData {
    #[cfg(feature = "sqlite")]
    /// Connects to the "map.sqlite" database.
    ///
    /// If the `blocks` table does not exist, tries to create it.
    ///
    /// ```
    /// # #[tokio::main]
    /// # async fn main() {
    /// use minetestworld::MapData;
    ///
    /// let meta = MapData::from_sqlite_file("TestWorld/map.sqlite", false).await.unwrap();
    /// # }
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
                Ok(MapData::Sqlite {
                    connection: pool.clone(),
                    split_blockpos: !sqlite_column_exists(pool, "pos").await?,
                })
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
        let port = port.map(|p| format!(":{p}")).unwrap_or_default();
        let url = format!("redis://{host}{port}/");

        Ok(MapData::Redis {
            connection: redis::Client::open(url)?
                .get_multiplexed_tokio_connection()
                .await?,
            hash: hash.to_string(),
        })
    }

    #[cfg(feature = "leveldb")]
    /// Opens a local LevelDB database
    pub fn from_leveldb(leveldb_directory: impl AsRef<Path>) -> Result<MapData, MapDataError> {
        let options = LdbOptions { create_if_missing: true, ..Default::default() }; // Is currently the default, but just to be sure
        let db = LdbCon::open(leveldb_directory, options).map_err(MapDataError::LevelDbError)?;
        Ok(MapData::LevelDb(Arc::new(Mutex::new(db))))
    }

    /// Returns the positions of all mapblocks
    ///
    /// Note that the unit of the coordinates will be
    /// [MAPBLOCK_LENGTH][`crate::map_block::MAPBLOCK_LENGTH`].
    pub async fn all_mapblock_positions<'a>(
        &'a self,
    ) -> BoxStream<'a, Result<Position, MapDataError>> {
        match self {
            #[cfg(feature = "sqlite")]
            MapData::Sqlite {
                connection,
                split_blockpos,
            } => stream_sqlite_positions(connection, *split_blockpos).await,
            #[cfg(feature = "postgres")]
            MapData::Postgres(pool) => sqlx::query_as("SELECT posx, posy, posz FROM blocks")
                .fetch(pool)
                .map_err(MapDataError::SqlError)
                .boxed(),
            #[cfg(feature = "redis")]
            MapData::Redis { connection, hash } => {
                // We can't really stream, so we'll just collect the result with hkeys
                let positions: Result<Vec<i64>, _> =
                    connection.clone().hkeys(hash.to_string()).await; // might block long
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
            #[cfg(feature = "leveldb")]
            MapData::LevelDb(db) =>
            // TODO Use task::spawn_blocking for this, as this blocks the thread for a longer time
            {
                match db.clone().lock_owned().await.new_iter() {
                    Ok(mut iter) => {
                        let positions: Vec<_> = std::iter::from_fn(move || iter.next())
                            .map(|(key, _value)| {
                                String::from_utf8_lossy(&key)
                                    .parse::<i64>()
                                    .map(Position::from_database_key)
                            })
                            .collect(); // might block long
                        stream::iter(positions)
                            .map_err(MapDataError::ParseIntError)
                            .boxed()
                    }
                    Err(e) => {
                        stream::once(future::ready(Err(MapDataError::LevelDbError(e)))).boxed()
                    }
                }
            }
        }
    }

    /// Queries the backend for the data of a single mapblock
    pub async fn get_block_data(&self, pos: Position) -> Result<Vec<u8>, MapDataError> {
        let pos_index = pos.as_database_key();
        match self {
            #[cfg(feature = "sqlite")]
            MapData::Sqlite {
                connection,
                split_blockpos,
            } => get_sqlite_data_query(&pos, *split_blockpos)
                .fetch_one(connection)
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
            #[cfg(feature = "leveldb")]
            MapData::LevelDb(db) => {
                let mut db = db.lock().await;
                let snapshot = db.get_snapshot();
                db.get_at(&snapshot, format!("{}", pos.as_database_key()).as_bytes())
                    .map_err(MapDataError::LevelDbError)
                    .transpose()
                    .unwrap_or_else(|| Err(MapDataError::MapBlockNonexistent(pos)))
                    .map(|b| b.to_vec())
            }
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
            MapData::Sqlite {
                connection,
                split_blockpos,
            } => prepare_sqlite_upsert_query(&pos, *split_blockpos)
                .bind(data)
                .execute(connection)
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
            #[cfg(feature = "leveldb")]
            MapData::LevelDb(db) => {
                let mut db = db.clone().lock_owned().await;
                db.put(format!("{}", pos.as_database_key()).as_bytes(), data)?;
                Ok(())
            }
        }
    }

    /// Inserts or replaces the map block at `pos`
    pub async fn set_mapblock(&self, pos: Position, block: &MapBlock) -> Result<(), MapDataError> {
        self.set_mapblock_data(pos, &block.to_binary()?).await
    }
}

/// Checks if the `blocks` table contains the specified column
async fn sqlite_column_exists(pool: SqlitePool, column_name: &str) -> Result<bool, sqlx::Error> {
    Ok(sqlx::query("PRAGMA table_info(blocks)")
        .fetch_all(&pool)
        .await?
        .iter()
        .map::<String, _>(|col| col.get("name"))
        .any(|colname| colname == column_name))
}

async fn stream_sqlite_positions<'a>(
    connection: &'a SqlitePool,
    split_pos: bool,
) -> BoxStream<'a, Result<Position, MapDataError>> {
    if split_pos {
        sqlx::query("SELECT x, y, z FROM blocks")
            .try_map(|row: SqliteRow| {
                Ok(Position {
                    x: row.try_get("x")?,
                    y: row.try_get("y")?,
                    z: row.try_get("z")?,
                })
            })
            .fetch(connection)
            .map_err(MapDataError::SqlError)
            .boxed()
    } else {
        sqlx::query("SELECT pos FROM blocks")
            .try_map(|row: SqliteRow| Ok(Position::from_database_key(row.try_get("pos")?)))
            .fetch(connection)
            .map_err(MapDataError::SqlError)
            .boxed()
    }
}

/// Converts a position into an SQLite statement which queries the block data
#[cfg(feature = "sqlite")]
fn get_sqlite_data_query<'a>(
    pos: &'a Position,
    split_pos: bool,
) -> Query<'a, Sqlite, SqliteArguments<'a>> {
    if split_pos {
        sqlx::query("SELECT data FROM blocks WHERE x = ? AND y = ? AND z = ?")
            .bind(pos.x)
            .bind(pos.y)
            .bind(pos.z)
    } else {
        sqlx::query("SELECT data FROM blocks WHERE pos = ?").bind(pos.as_database_key())
    }
}

/// Returns a prepared SQLite statement for setting block data. Insert the data with `.bind()` before executing.
#[cfg(feature = "sqlite")]
fn prepare_sqlite_upsert_query<'a>(
    pos: &'a Position,
    split_pos: bool,
) -> Query<'a, Sqlite, SqliteArguments<'a>> {
    if split_pos {
        sqlx::query(SQLITE_UPSERT)
            .bind(pos.x)
            .bind(pos.y)
            .bind(pos.z)
    } else {
        sqlx::query(SQLITE_UPSERT_UNIFIED_POS).bind(pos.as_database_key())
    }
}
