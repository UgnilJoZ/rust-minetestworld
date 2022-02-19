use crate::positions::{get_integer_as_block, Position, mapblock_node_position};
use rusqlite::{Connection, NO_PARAMS};
use std::collections::HashMap;
use std::io::Read;

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

/// A 'chunk' of nodes and the smallest unit saved in a backend
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
        let map_format_version = read_u8(&mut data)?;
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

    pub fn content_from_id(&self, content_id: u16) -> &[u8] {
        self.name_id_mappings
            .get(&content_id)
            .map(|v| v.as_slice())
            .unwrap_or(b"unkown")
    }
}

pub struct NodeIter {
    mapblock: MapBlock,
    mapblock_position: Position,
    node_index: u16,
}

impl NodeIter {
    pub(crate) fn new(mapblock: MapBlock, mapblock_position: Position) -> Self {
        NodeIter {
            mapblock,
            mapblock_position,
            node_index: 0,
        }
    }
}

/// A single voxel
#[derive(Debug)]
pub struct Node {
    /// Content type
    pub param0: String,
    /// Lighting
    pub param1: u8,
    /// Additional data
    pub param2: u8,
}

impl Iterator for NodeIter {
    type Item = (Position, Node);

    fn next(&mut self) -> Option<Self::Item> {
        let index = self.node_index;
        if index < 4096 {
            self.node_index += 1;
            let pos = self.mapblock_position + mapblock_node_position(index);
            let param0 = self
                .mapblock
                .content_from_id(self.mapblock.param0[index as usize]);
            let node = Node {
                param0: String::from_utf8_lossy(param0).into_owned(),
                param1: self.mapblock.param1[index as usize],
                param2: self.mapblock.param2[index as usize],
            };
            Some((pos, node))
        } else {
            None
        }
    }
}