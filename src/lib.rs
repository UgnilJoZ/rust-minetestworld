extern crate rusqlite;
use rusqlite::{Connection, OpenFlags, NO_PARAMS};
use std::collections::HashMap;
use std::io::Read;
use std::path::Path;

#[derive(Debug, PartialEq, Copy, Clone)]
pub struct Position {
    x: i16,
    y: i16,
    z: i16,
}

// While there is no modulo operator in rust, we'll use the remainder operator (%) to build one.
fn modulo(a: i64, b: i64) -> i64 {
    ((a % b + b) % b) as i64
}

fn get_integer_as_block(i: i64) -> Position {
    fn unsigned_to_signed(i: i64, max_positive: i64) -> i64 {
        if i < max_positive {
            i
        } else {
            i - 2 * max_positive
        }
    }

    let x = unsigned_to_signed(modulo(i, 4096), 2048) as i16;
    let mut i = (i - x as i64) / 4096;
    let y = unsigned_to_signed(modulo(i, 4096), 2048) as i16;
    i = (i - y as i64) / 4096;
    let z = unsigned_to_signed(modulo(i, 4096), 2048) as i16;
    Position { x, y, z }
}

fn get_block_as_integer(p: Position) -> i64 {
    p.x as i64 + p.y as i64 * 4096 + p.z as i64 * 16777216
}

fn read_u8(r: &mut impl Read) -> Result<u8, std::io::Error> {
    let mut buf = [0; 1];
    r.read_exact(&mut buf)?;
    Ok(buf[0])
}

fn read_u16_be(r: &mut impl Read) -> std::io::Result<u16> {
    let mut buffer = [0; 2];
    r.read_exact(&mut buffer)?;
    Ok(u16::from_be_bytes(buffer))
}

fn read_u32_be(r: &mut impl Read) -> std::io::Result<u32> {
    let mut buffer = [0; 4];
    r.read_exact(&mut buffer)?;
    Ok(u32::from_be_bytes(buffer))
}

pub fn get_all_positions(conn: &Connection) -> Result<Vec<Position>, rusqlite::Error> {
    let mut stmt = conn.prepare("SELECT pos FROM blocks")?;
    let result = stmt.query_map(NO_PARAMS, |row| row.get(0).map(get_integer_as_block))?;
    result.collect()
}

fn get_block_data(conn: &Connection, pos: Position) -> Result<Vec<u8>, rusqlite::Error> {
    let pos = get_block_as_integer(pos);
    conn.query_row("SELECT data FROM blocks WHERE pos = ?", &[pos], |row| {
        row.get(0)
    })
}

#[derive(thiserror::Error, Debug)]
pub enum MapBlockError {
    #[error("MapBlock malformed: {0}")]
    BlobMalformed(String),
    #[error("Read error: {0}")]
    ReadError(#[from] std::io::Error),
    #[error("Wrong mapblock format: Version {0} is not supported")]
    MapVersionError(u8),
    #[error("Sqlite error: {0}")]
    SQLiteError(#[from] rusqlite::Error),
}

pub struct NodeMetadata {
    pub position: u16,
    pub vars: HashMap<String, Vec<u8>>,
}

pub struct StaticObject {
    pub type_id: u8,
    pub x: i32,
    pub y: i32,
    pub z: i32,
    pub data: Vec<u8>,
}

pub struct NodeTimer {
    pub timer_position: u16,
    pub timeout: i32,
    pub elapsed: i32,
}

pub struct MapBlock {
    pub map_format_version: u8,
    pub flags: u8,
    pub lighting_complete: u16,
    pub timestamp: u32,
    pub name_id_mappings: HashMap<u16, Vec<u8>>,
    pub content_width: u8,
    pub params_width: u8,
    pub param0: [u16; 4096],
    pub param1: [u8; 4096],
    pub param2: [u8; 4096],
    pub node_metadata: Vec<NodeMetadata>,
    pub static_object_version: u8,
    pub static_objects: Vec<StaticObject>,
}

impl MapBlock {
    pub fn from_data<R: Read>(mut data: R) -> Result<MapBlock, MapBlockError> {
        let map_format_version = read_u8(&mut data)
            .map_err(|_| MapBlockError::BlobMalformed("Cannot read block data".to_string()))?;
        if map_format_version != 29 {
            return Err(MapBlockError::MapVersionError(map_format_version));
        }
        // Read all into a vector
        let mut buffer = vec![];
        let mut zstd = zstd::stream::Decoder::new(data)
            .map_err(|_| MapBlockError::BlobMalformed("Zstd error".to_string()))?;
        zstd.read_to_end(&mut buffer)
            .map_err(|_| MapBlockError::BlobMalformed("Zstd error".to_string()))?;
        let mut data = buffer.as_slice();

        let flags = read_u8(&mut data)?;

        let lighting_complete = read_u16_be(&mut data)?;

        let timestamp = read_u32_be(&mut data)?;

        if read_u8(&mut data)? != 0 {
            return Err(MapBlockError::BlobMalformed(
                "name_id_mappings version byte is not zero".to_owned(),
            ));
        }

        let num_name_id_mappings = read_u16_be(&mut data)?;
        let mut name_id_mappings = HashMap::new();
        for _ in 0..num_name_id_mappings {
            let id = read_u16_be(&mut data)?;
            let mut name = vec![0; read_u16_be(&mut data)? as usize];
            data.read_exact(&mut name)?;

            if let Some(old_name) = name_id_mappings.insert(id, name.clone()) {
                return Err(MapBlockError::BlobMalformed(format!(
                    "Node ID {id} appears multiple times in name_id_mappings: {} and {}",
                    String::from_utf8_lossy(&old_name),
                    String::from_utf8_lossy(&name)
                )));
            }
        }

        let content_width = read_u8(&mut data)?;
        if content_width != 2 {
            return Err(MapBlockError::BlobMalformed(format!(
                "\"{content_width}\" is not the expected content_width"
            )));
        }

        let params_width = read_u8(&mut data)?;
        if params_width != 2 {
            return Err(MapBlockError::BlobMalformed(format!(
                "\"{params_width}\" is not the expected params_width"
            )));
        }

        let mut mapblock = MapBlock {
            map_format_version,
            flags,
            lighting_complete,
            timestamp,
            name_id_mappings,
            content_width,
            params_width,
            param0: [0; 4096],
            param1: [0; 4096],
            param2: [0; 4096],
            node_metadata: vec![],
            static_object_version: 0,
            static_objects: vec![],
        };

        for p0 in mapblock.param0.iter_mut() {
            *p0 = read_u16_be(&mut data)?;
        }

        data.read_exact(&mut mapblock.param1)?;
        data.read_exact(&mut mapblock.param2)?;

        // TODO node metadata, static objects

        Ok(mapblock)
    }

    pub fn from_sqlite(conn: &Connection, pos: Position) -> Result<MapBlock, MapBlockError> {
        MapBlock::from_data(get_block_data(conn, pos)?.as_slice())
    }
}

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
}

#[cfg(test)]
mod tests {
    use get_all_positions;
    use get_block_data;
    use get_integer_as_block;
    use rusqlite::Connection;
    use rusqlite::OpenFlags;
    use MapBlock;
    use Position;
    #[test]
    fn db_exists() {
        Connection::open_with_flags("test.sqlite", OpenFlags::SQLITE_OPEN_READ_ONLY).unwrap();
    }

    #[test]
    fn can_query() {
        let conn =
            Connection::open_with_flags("test.sqlite", OpenFlags::SQLITE_OPEN_READ_ONLY).unwrap();
        assert_eq!(get_all_positions(&conn).unwrap().len(), 5923);
        let block = get_block_data(
            &conn,
            Position {
                x: -13,
                y: -8,
                z: 2,
            },
        )
        .unwrap();
        assert_eq!(block.len(), 40);
    }

    #[test]
    fn simple_math() {
        assert_eq!(
            get_integer_as_block(134270984),
            Position { x: 8, y: 13, z: 8 }
        );
        assert_eq!(
            get_integer_as_block(-184549374),
            Position { x: 2, y: 0, z: -11 }
        );
    }

    #[test]
    fn can_parse_mapblock() {
        MapBlock::from_data(std::fs::File::open("testmapblock").unwrap()).unwrap();
    }

    #[test]
    fn can_parse_all_mapblocks() {
        let conn =
            Connection::open_with_flags("test.sqlite", OpenFlags::SQLITE_OPEN_READ_ONLY).unwrap();
        let blocks: Vec<_> = get_all_positions(&conn)
            .unwrap()
            .into_iter()
            .map(|pos| MapBlock::from_sqlite(&conn, pos))
            .collect();
        let succeeded = blocks.iter().filter(|b| b.is_ok()).count();
        let failed = blocks.iter().filter(|b| b.is_err()).count();
        eprintln!("Succeeded parsed blocks: {succeeded}\nFailed blocks: {failed}");
        assert_eq!(failed, 0);
    }
}
