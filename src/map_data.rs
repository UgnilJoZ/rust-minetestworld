use rusqlite::{Connection, OpenFlags};
use std::path::Path;

use crate::map_block::{get_all_positions, MapBlock, MapBlockError, NodeIter};
use crate::positions::{get_block_as_integer, Position};

#[derive(thiserror::Error, Debug)]
pub enum MapDataError {
    #[error("Sqlite error: {0}")]
    SqliteError(#[from] rusqlite::Error),
    #[error("MapBlockError: {0}")]
    MapBlockError(#[from] MapBlockError),
}

pub enum MapData {
    Sqlite(Connection),
}

impl MapData {
    pub fn from_sqlite_file<P: AsRef<Path>>(filename: P) -> Result<MapData, MapDataError> {
        Ok(MapData::Sqlite(Connection::open_with_flags(
            filename,
            OpenFlags::SQLITE_OPEN_READ_ONLY,
        )?))
    }

    pub fn all_mapblock_positions(&self) -> Result<Vec<Position>, MapDataError> {
        match self {
            MapData::Sqlite(con) => Ok(get_all_positions(con)?),
        }
    }

    pub(crate) fn get_block_data(&self, pos: Position) -> Result<Vec<u8>, rusqlite::Error> {
        let pos = get_block_as_integer(pos);
        match self {
            MapData::Sqlite(con) => {
                con.query_row("SELECT data FROM blocks WHERE pos = ?", &[pos], |row| {
                    row.get(0)
                })
            }
        }
    }

    pub fn get_mapblock(&self, pos: Position) -> Result<MapBlock, MapDataError> {
        Ok(MapBlock::from_data(self.get_block_data(pos)?.as_slice())?)
    }

    /// Enumerate all nodes from the mapblock at `pos`
    pub fn iter_mapblock_nodes(&self, mapblock_pos: Position) -> Result<NodeIter, MapDataError> {
        let data = self.get_block_data(mapblock_pos)?;
        let mapblock = MapBlock::from_data(data.as_slice())?;
        Ok(NodeIter::new(mapblock, mapblock_pos))
    }
}
