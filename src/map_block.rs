//! Contains data types and constants to work with MapBlocks

use crate::positions::{mapblock_node_index, mapblock_node_position, Position};

use std::collections::HashMap;
use std::io::Read;

#[cfg(feature = "smartstring")]
type String = smartstring::SmartString<smartstring::LazyCompact>;

/// Side length of map blocks.
///
/// The map data is divided into chunks of nodes (=voxels).
/// Currently, a chunk consists of 16·16·16 nodes.
///
/// The number of nodes a mapblock contains is [`MAPBLOCK_SIZE`].
///
/// ```
/// use minetestworld::MAPBLOCK_LENGTH;
///
/// assert_eq!(MAPBLOCK_LENGTH, 16);
/// ```
pub const MAPBLOCK_LENGTH: u8 = 16;

/// How many nodes are contained in a map block.
///
/// This is [`MAPBLOCK_LENGTH`]³.
///
/// ```
/// use minetestworld::MAPBLOCK_SIZE;
///
/// assert_eq!(MAPBLOCK_SIZE, 4096);
/// ```
pub const MAPBLOCK_SIZE: usize =
    MAPBLOCK_LENGTH as usize * MAPBLOCK_LENGTH as usize * MAPBLOCK_LENGTH as usize;

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

/// An error during the decoding of a MapBlock
#[derive(thiserror::Error, Debug)]
pub enum MapBlockError {
    /// The mapblock did not follow the expected binary structure.
    ///
    /// This variant contains a more detailed error message.
    #[error("MapBlock malformed: {0}")]
    BlobMalformed(String),
    #[error("Read error: {0}")]
    /// The underlying reader returned an error, which is contained.
    ReadError(#[from] std::io::Error),
    /// The mapblock does not have a current enough version.
    ///
    /// The 'wrong' version is contained.
    #[error("Wrong mapblock format: Version {0} is not supported")]
    MapVersionError(u8),
}

/// Metadata of a node
/// 
/// e.g. the inventory of a chest or the text of a sign
pub struct NodeMetadata {
    /// The node index in the flat node array
    pub position: u16,
    /// A dictionary containing the metadata values
    pub vars: HashMap<String, Vec<u8>>,
}

/// Objects in the world that are not nodes
/// 
/// For example a LuaEntity
pub struct StaticObject {
    /// Type ID
    pub type_id: u8,
    /// x coordinate
    pub x: i32,
    /// y coordinate
    pub y: i32,
    /// z coordinate
    pub z: i32,
    /// The object's data
    pub data: Vec<u8>,
}

/// Represents a running node timer
pub struct NodeTimer {
    ///The node index in the flat node array
    pub timer_position: u16,
    /// Timeout in milliseconds
    pub timeout: i32,
    /// Elapsed time in milliseconds
    pub elapsed: i32,
}

/// A 'chunk' of nodes and the smallest unit saved in a backend
///
/// Refer to <https://github.com/minetest/minetest/blob/master/doc/world_format.txt>
pub struct MapBlock {
    /// The format version of the mapblock. Currently supported is only version 29.
    ///
    /// An attempt to read a block of a previous version will result in a
    /// [`MapBlockError::MapVersionError`].
    pub map_format_version: u8,
    /// Flags telling if this chunk is underground etc.
    pub flags: u8,
    /// Flags that indicate if the lighting is complete at each side.
    pub lighting_complete: u16,
    /// Timestamp of last save , in seconds from game start
    pub timestamp: u32,
    /// Maps each numeric content ID to the content name.
    ///
    /// This is used to efficiently store nodes.
    pub name_id_mappings: HashMap<u16, Vec<u8>>,
    /// Number bytes used for the content (param0) field of the nodes
    pub content_width: u8,
    /// Additional node params, always 2
    pub params_width: u8,
    /// The content ID of each node in the mapblock.
    ///
    /// It can be mapped to names via [`MapBlock::name_id_mappings`]
    pub param0: [u16; 4096],
    /// The param1 field of every node
    pub param1: [u8; 4096],
    /// The param2 field of every node
    pub param2: [u8; 4096],
    /// Nodfe metadata
    pub node_metadata: Vec<NodeMetadata>,
    /// Static object version
    pub static_object_version: u8,
    /// Objects that are no nodes
    pub static_objects: Vec<StaticObject>,
}

impl MapBlock {
    /// Constructs a Mapblock from its binary representation
    pub fn from_data<R: Read>(mut data: R) -> Result<MapBlock, MapBlockError> {
        let map_format_version = read_u8(&mut data)?;
        if map_format_version != 29 {
            return Err(MapBlockError::MapVersionError(map_format_version));
        }
        // Read all into a vector
        let mut buffer = vec![];
        let mut zstd = zstd::stream::Decoder::new(data)
            .map_err(|_| MapBlockError::BlobMalformed("Zstd error".to_string().into()))?;
        zstd.read_to_end(&mut buffer)
            .map_err(|_| MapBlockError::BlobMalformed("Zstd error".to_string().into()))?;
        let mut data = buffer.as_slice();

        let flags = read_u8(&mut data)?;

        let lighting_complete = read_u16_be(&mut data)?;

        let timestamp = read_u32_be(&mut data)?;

        if read_u8(&mut data)? != 0 {
            return Err(MapBlockError::BlobMalformed(
                "name_id_mappings version byte is not zero"
                    .to_owned()
                    .into(),
            ));
        }

        let num_name_id_mappings = read_u16_be(&mut data)?;
        let mut name_id_mappings = HashMap::new();
        for _ in 0..num_name_id_mappings {
            let id = read_u16_be(&mut data)?;
            let mut name = vec![0; read_u16_be(&mut data)? as usize];
            data.read_exact(&mut name)?;

            if let Some(old_name) = name_id_mappings.insert(id, name.clone()) {
                return Err(MapBlockError::BlobMalformed(
                    format!(
                        "Node ID {id} appears multiple times in name_id_mappings: {} and {}",
                        std::string::String::from_utf8_lossy(&old_name),
                        std::string::String::from_utf8_lossy(&name)
                    )
                    .into(),
                ));
            }
        }

        let content_width = read_u8(&mut data)?;
        if content_width != 2 {
            return Err(MapBlockError::BlobMalformed(
                format!("\"{content_width}\" is not the expected content_width").into(),
            ));
        }

        let params_width = read_u8(&mut data)?;
        if params_width != 2 {
            return Err(MapBlockError::BlobMalformed(
                format!("\"{params_width}\" is not the expected params_width").into(),
            ));
        }

        let mut mapblock = MapBlock {
            map_format_version,
            flags,
            lighting_complete,
            timestamp,
            name_id_mappings,
            content_width,
            params_width,
            param0: [0; MAPBLOCK_SIZE],
            param1: [0; MAPBLOCK_SIZE],
            param2: [0; MAPBLOCK_SIZE],
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

    /// Gets the content type string from a content ID
    pub fn content_from_id(&self, content_id: u16) -> &[u8] {
        self.name_id_mappings
            .get(&content_id)
            .map(|v| v.as_slice())
            .unwrap_or(b"unkown")
    }

    /// Queries the mapblock for a node on the given relative coordinates
    pub fn get_node_at(&self, x: u8, y: u8, z: u8) -> Node {
        let index = mapblock_node_index(x, y, z) as usize;
        let param0 = self.content_from_id(self.param0[index as usize]);
        Node {
            param0: std::string::String::from_utf8_lossy(param0).into(),
            param1: self.param1[index],
            param2: self.param2[index],
        }
    }
}

/// Iterates through the nodes in a mapblock.
///
/// This yields a tuple in the form ([relative_position][`Position`],
/// [node][`Node`]).
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

impl Iterator for NodeIter {
    /// The type this iterator yields.
    ///
    /// This is a tuple consisting if the node and its relative position in the chunk.
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
                param0: std::string::String::from_utf8_lossy(param0).into(),
                param1: self.mapblock.param1[index as usize],
                param2: self.mapblock.param2[index as usize],
            };
            Some((pos, node))
        } else {
            None
        }
    }
}
