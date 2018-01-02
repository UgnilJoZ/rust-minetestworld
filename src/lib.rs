extern crate rusqlite;
use rusqlite::Connection;
use std::collections::HashMap;
use std::io::Read;
extern crate flate2;
use flate2::read::ZlibDecoder;

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

fn getIntegerAsBlock(i: i64) -> Position {
	fn unsignedToSigned(i: i64, max_positive: i64) -> i64 {
		if i < max_positive
			{ i }
		else
			{ i - 2 * max_positive }
	}

	let x = unsignedToSigned(modulo(i, 4096), 2048) as i16;
	let mut i = (i - x as i64) / 4096;
	let y = unsignedToSigned(modulo(i, 4096), 2048) as i16;
	i = (i - y as i64) / 4096;
	let z = unsignedToSigned(modulo(i, 4096), 2048) as i16;
	return Position {x,y,z}
}

fn getBlockAsInteger(p: Position) -> i64 {
	p.x as i64 + p.y as i64 * 4096 + p.z as i64 * 16777216
}

fn getAllPositions(conn: &Connection) -> Result<Vec<Position>, rusqlite::Error> {
	match conn.prepare("SELECT pos FROM blocks") {
		Ok(mut stmt) =>
			match stmt.query_map(&[], |row| { getIntegerAsBlock(row.get(0)) }) {
				Ok(pos_iter) => pos_iter.collect(),
				Err(e) => Err(e),
			},
		Err(e) => Err(e),
	}
}

fn getBlockData(conn: &Connection, pos: Position) -> Result<Vec<u8>, rusqlite::Error> {
	conn.query_row("SELECT data FROM blocks WHERE pos = ?", &[&getBlockAsInteger(pos)], |row| row.get(0))
}

#[derive(Debug)]
pub enum MapBlockError {
	BlobMalformed(String),
	MapVersionError(u8),
	SQLiteError(rusqlite::Error),
}

pub struct NodeMetadata {
	position: u16,
	vars: HashMap<String, Vec<u8>>,
}

pub struct StaticObject {
	type_id: u8,
	x: i32,
	y: i32,
	z: i32,
	data: Vec<u8>,
}

pub struct NodeTimer {
	timer_position: u16,
	timeout: i32,
	elapsed: i32,
}

pub struct MapBlock {
	map_format_version: u8,
	flags: u8,
	lightning: u16,
	content_width: u8,
	params_width: u8,
	param0: [u16; 4096],
	param1: [u8; 4096],
	param2: [u8; 4096],
	node_metadata: Vec<NodeMetadata>,
	static_object_version: u8,
	static_objects: Vec<StaticObject>,
	timestamp: u32,
	name_id_mappings: Vec<(u16, String)>,
	node_timers: Vec<NodeTimer>,
}

impl MapBlock {
	pub fn from_data<R: Read>(mut data: R) -> Result<MapBlock, MapBlockError> {
		// Read first few fields
		let mut buffer = [0; 6];
		if data.read_exact(&mut buffer).is_err() {
			return Err(MapBlockError::BlobMalformed("Block data is way too short".to_string()));
		}
		let mut block = MapBlock {
			map_format_version: buffer[0],
			flags: buffer[1],
			lightning: buffer[2] as u16 * 256 + buffer[3] as u16,
			content_width: buffer[4],
			params_width: buffer[5],
			param0: [0; 4096],
			param1: [0; 4096],
			param2: [0; 4096],
			node_metadata: vec!(),
			static_object_version: 0,
			static_objects: vec!(),
			timestamp: 0xffffffff,
			name_id_mappings: vec!(),
			node_timers: vec!(),
		};
		if block.map_format_version != 28 {
			return Err(MapBlockError::MapVersionError(block.map_format_version));
		}

		// Read param0 + param1 + param2
		let mut decompressor = ZlibDecoder::new(data);
		let mut buffer = [0; 8192];
		let (p0, p1, p2) = (decompressor.read_exact(&mut buffer), decompressor.read_exact(&mut block.param1), decompressor.read_exact(&mut block.param2));
		if p0.is_err() || p1.is_err() || p2.is_err() {
			return Err(MapBlockError::BlobMalformed("Block data is too short to read param_n".to_string()));
		}
		//if decompressor.bytes().count() != 0 {
		//	return Err(MapBlockError::BlobMalformed("zlib-compressed data for param_n contains too much data.".to_string()));
		//}
		data = decompressor.into_inner();
		
		// Read mapblock metadata
		let mut decompressor = ZlibDecoder::new(data);
		let mut buffer = [0; 3];
		if decompressor.read_exact(&mut buffer).is_err() {
			return Err(MapBlockError::BlobMalformed("Block data is too short to read metadata".to_string()));
		}

		return Ok(block);
	}

	pub fn from_sqlite(conn: &Connection, pos: Position) -> Result<MapBlock, MapBlockError> {
		match getBlockData(conn, pos) {
			Ok(blob) => MapBlock::from_data(blob.as_slice()),
			Err(e) => Err(MapBlockError::SQLiteError(e)),
		}
	}
}

#[cfg(test)]
mod tests {
	use rusqlite::Connection;
	use rusqlite::OpenFlags;
	use getIntegerAsBlock;
	use getAllPositions;
	use MapBlock;
	use Position;
	use getBlockData;
	#[test]
	fn db_exists() {
		Connection::open_with_flags("test.sqlite", OpenFlags::SQLITE_OPEN_READ_ONLY).unwrap();
	}
	
	#[test]
	fn can_query() {
		let conn = Connection::open_with_flags("test.sqlite", OpenFlags::SQLITE_OPEN_READ_ONLY).unwrap();
		assert_eq!(getAllPositions(&conn).unwrap().len(), 8398);
		let block = getBlockData(&conn, Position { x: 8, y: 13, z: 8 }).unwrap();
		assert_eq!(block.len(), 77);
	}

	#[test]
	fn simple_math() {
		assert_eq!(getIntegerAsBlock(134270984), Position { x: 8, y: 13, z: 8 });
	}

	#[test]
	fn can_parse_mapblock() {
		let conn = Connection::open_with_flags("test.sqlite", OpenFlags::SQLITE_OPEN_READ_ONLY).unwrap();
		let block = MapBlock::from_sqlite(&conn, Position { x: 8, y: 13, z: 8 }).unwrap();
		assert_eq!(block.map_format_version, 28);
	}
}
