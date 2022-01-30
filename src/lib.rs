extern crate rusqlite;
use rusqlite::{Connection, NO_PARAMS};
use std::collections::HashMap;
use std::io::Read;

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

#[derive(Debug)]
pub enum MapBlockError {
    BlobMalformed(String),
    MapVersionError(u8),
    SQLiteError(rusqlite::Error),
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
    pub lightning: u16,
    pub content_width: u8,
    pub params_width: u8,
    pub param0: [u16; 4096],
    pub param1: [u8; 4096],
    pub param2: [u8; 4096],
    pub node_metadata: Vec<NodeMetadata>,
    pub static_object_version: u8,
    pub static_objects: Vec<StaticObject>,
    pub timestamp: u32,
    pub name_id_mappings: Vec<(u16, String)>,
    pub node_timers: Vec<NodeTimer>,
}

impl MapBlock {
    pub fn from_data<R: Read>(mut data: R) -> Result<MapBlock, MapBlockError> {
        let map_version = read_u8(&mut data)
            .map_err(|_| MapBlockError::BlobMalformed("Cannot read block data".to_string()))?;
        if map_version != 29 {
            return Err(MapBlockError::MapVersionError(map_version));
        }
        // Read all into a vector
        let mut buffer = vec![];
        let mut zstd = zstd::stream::Decoder::new(data)
            .map_err(|_| MapBlockError::BlobMalformed("Zstd error".to_string()))?;
        zstd.read_to_end(&mut buffer)
            .map_err(|_| MapBlockError::BlobMalformed("Zstd error".to_string()))?;
        let mut data = buffer.as_slice();
        // Read first few fields
        let mut buffer = [0; 6];

        if data.read_exact(&mut buffer).is_err() {
            return Err(MapBlockError::BlobMalformed(
                "Block data is way too short".to_string(),
            ));
        }
        let mut block = MapBlock {
            map_format_version: map_version,
            flags: buffer[0],
            lightning: buffer[1] as u16 * 256 + buffer[2] as u16,
            content_width: buffer[3],
            params_width: buffer[4],
            param0: [0; 4096],
            param1: [0; 4096],
            param2: [0; 4096],
            node_metadata: vec![],
            static_object_version: 0,
            static_objects: vec![],
            timestamp: 0xffffffff,
            name_id_mappings: vec![],
            node_timers: vec![],
        };

        // Read param0 + param1 + param2
        let mut buffer = [0; 8192];
        let (p0, p1, p2) = (
            data.read_exact(&mut buffer),
            data.read_exact(&mut block.param1),
            data.read_exact(&mut block.param2),
        );
        if p0.is_err() || p1.is_err() || p2.is_err() {
            return Err(MapBlockError::BlobMalformed(
                "Block data is too short to read param_n".to_string(),
            ));
        }

        // Save param0
        for c in buffer.chunks(2).take(4096).enumerate() {
            let index = c.0;
            let bytes = c.1;
            block.param0[index] = bytes[0] as u16 * 256 + bytes[1] as u16;
        }

        // Read mapblock metadata
        let mut buffer = [0; 3];
        if data.read_exact(&mut buffer).is_err() {
            return Err(MapBlockError::BlobMalformed(
                "Block data is too short to read metadata".to_string(),
            ));
        }

        Ok(block)
    }

    pub fn from_sqlite(conn: &Connection, pos: Position) -> Result<MapBlock, MapBlockError> {
        match get_block_data(conn, pos) {
            Ok(blob) => MapBlock::from_data(blob.as_slice()),
            Err(e) => Err(MapBlockError::SQLiteError(e)),
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
