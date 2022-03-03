use futures::stream::TryStreamExt;
use redis::aio::MultiplexedConnection as RedisConn;
use redis::AsyncCommands;
use sqlx::prelude::*;
use sqlx::sqlite::{SqliteConnectOptions, SqlitePool};
use std::path::Path;
use url::Host;

#[cfg(feature = "smartstring")]
use smartstring::alias::String;

use crate::map_block::{MapBlock, MapBlockError, Node, NodeIter};
use crate::positions::{get_block_as_integer, get_integer_as_block, Position};

#[derive(thiserror::Error, Debug)]
pub enum MapDataError {
    #[error("Database error: {0}")]
    SqlError(#[from] sqlx::Error),
    #[error("Database error: {0}")]
    RedisError(#[from] redis::RedisError),
    #[error("MapBlockError: {0}")]
    MapBlockError(#[from] MapBlockError),
}

pub enum MapData {
    Sqlite(SqlitePool),
    Redis { connection: RedisConn, hash: String },
}

impl MapData {
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
    pub async fn from_sqlite_file<P: AsRef<Path>>(filename: P) -> Result<MapData, MapDataError> {
        Ok(MapData::Sqlite(
            SqlitePool::connect_with(
                SqliteConnectOptions::new()
                    .immutable(true)
                    .filename(filename),
            )
            .await?,
        ))
    }

    pub async fn from_redis_connection_params(
        host: Host,
        port: Option<u16>,
        hash: String,
    ) -> Result<MapData, MapDataError> {
        Ok(MapData::Redis {
            connection: redis::Client::open(format!(
                "redis://{host}{}/",
                port.map(|p| format!(":{p}"))
                    .unwrap_or(std::string::String::new())
            ))?
            .get_multiplexed_async_std_connection()
            .await?,
            hash,
        })
    }

    /// Returns the positions of all mapblocks
    ///
    /// Note that the unit of the coordinates will be
    /// [MAPBLOCK_LENGTH][`crate::map_block::MAPBLOCK_LENGTH`].
    pub async fn all_mapblock_positions(&self) -> Result<Vec<Position>, MapDataError> {
        match self {
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
            MapData::Redis { connection, hash } => {
                let mut v: Vec<i64> = connection.clone().hkeys(hash.to_string()).await?;
                Ok(v.drain(..).map(|i| get_integer_as_block(i)).collect())
            }
        }
    }

    /// Queries the backend for the data of a single mapblock
    pub async fn get_block_data(&self, pos: Position) -> Result<Vec<u8>, MapDataError> {
        let pos = get_block_as_integer(pos);
        match self {
            MapData::Sqlite(pool) => Ok(sqlx::query("SELECT data FROM blocks WHERE pos = ?")
                .bind(pos)
                .fetch_one(pool)
                .await?
                .try_get("data")?),
            MapData::Redis { connection, hash } => {
                Ok(connection.clone().hget(hash.to_string(), pos).await?)
            }
        }
    }

    pub async fn get_mapblock(&self, pos: Position) -> Result<MapBlock, MapDataError> {
        Ok(MapBlock::from_data(
            self.get_block_data(pos).await?.as_slice(),
        )?)
    }

    /// Enumerate all nodes from the mapblock at `pos`
    pub async fn iter_mapblock_nodes(
        &self,
        mapblock_pos: Position,
    ) -> Result<impl Iterator<Item = (Position, Node)>, MapDataError> {
        let data = self.get_block_data(mapblock_pos).await?;
        let mapblock = MapBlock::from_data(data.as_slice())?;
        Ok(NodeIter::new(mapblock, mapblock_pos))
    }
}
