//! Contains data types and constants to work with MapBlocks

use crate::positions::Position;

use std::collections::HashMap;
use std::io::{Read, Write};

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

/// This content type string refers to an unknown content type
pub const CONTENT_UNKNOWN: &[u8] = b"unknown";

fn read_u8(r: &mut impl Read) -> std::io::Result<u8> {
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

fn read_i32_be(r: &mut impl Read) -> std::io::Result<i32> {
    let mut buffer = [0; 4];
    r.read_exact(&mut buffer)?;
    Ok(i32::from_be_bytes(buffer))
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

/// An error during the [decoding](`MapBlock::from_data`) of a MapBlock
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

    /// Node metadata version is not 2, hence unsupported
    #[error("Node metadata version {0} is not supported")]
    UnsupportedNodeMetadataVersion(u8),
}

/// Maps mapblock-local content IDs to content types
pub type NameIdMappings = HashMap<u16, Vec<u8>>;

#[derive(Debug)]
pub struct NodeVar {
    key: Vec<u8>,
    value: Vec<u8>,
    is_private: bool,
}

/// Metadata of a node
///
/// e.g. the inventory of a chest or the text of a sign
#[derive(Debug)]
pub struct NodeMetadata {
    /// The node index in the flat node array
    pub position: Position,
    /// Metadata variables
    pub vars: Vec<NodeVar>,
    /// Serialized inventory
    pub inventory: Vec<u8>,
}

/// Objects in the world that are not nodes
///
/// For example a LuaEntity
pub struct StaticObject {
    /// Type ID
    pub type_id: u8,
    /// x coordinate * 1000
    pub x: i32,
    /// y coordinate * 1000
    pub y: i32,
    /// z coordinate * 1000
    pub z: i32,
    /// The object's data
    pub data: Vec<u8>,
}

/// Represents a running node timer
pub struct NodeTimer {
    /// The node index in the flat node array
    pub position: Position,
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
    pub name_id_mappings: NameIdMappings,
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
    /// Node metadata
    pub node_metadata: Vec<NodeMetadata>,
    /// Objects that are no nodes
    pub static_objects: Vec<StaticObject>,
    /// Node timers
    pub node_timers: Vec<NodeTimer>,
}

impl MapBlock {
    /// Constructs a Mapblock from its binary representation
    pub fn from_data(mut data: impl Read) -> Result<MapBlock, MapBlockError> {
        let map_format_version = read_u8(&mut data)?;
        if map_format_version != 29 {
            return Err(MapBlockError::MapVersionError(map_format_version));
        }
        // Read all into a vector
        let mut buffer = vec![];
        zstd::stream::Decoder::new(data)?.read_to_end(&mut buffer)?;
        let mut data = buffer.as_slice();

        let flags = read_u8(&mut data)?;
        let lighting_complete = read_u16_be(&mut data)?;
        let timestamp = read_u32_be(&mut data)?;
        let name_id_mappings = read_name_id_mappings(&mut data)?;

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
            node_timers: vec![],
            static_objects: vec![],
        };

        for p0 in mapblock.param0.iter_mut() {
            *p0 = read_u16_be(&mut data)?;
        }

        data.read_exact(&mut mapblock.param1)?;
        data.read_exact(&mut mapblock.param2)?;

        mapblock.node_metadata = read_node_metadata(&mut data).unwrap();
        mapblock.static_objects = read_static_objects(&mut data).unwrap();
        mapblock.node_timers = read_timers(&mut data).unwrap();

        Ok(mapblock)
    }

    /// Serializes the map block into the binay format
    pub fn to_binary(&self) -> std::io::Result<Vec<u8>> {
        let mut encoder = zstd::stream::Encoder::new(vec![29], 0)?;

        encoder.write(&self.flags.to_be_bytes())?;
        encoder.write(&self.lighting_complete.to_be_bytes())?;
        encoder.write(&self.timestamp.to_be_bytes())?;
        write_name_id_mappings(&self.name_id_mappings, &mut encoder)?;

        encoder.write(&[2])?; // content_width
        encoder.write(&[2])?; // params_width

        for value in self.param0 {
            encoder.write(&value.to_be_bytes())?;
        }
        encoder.write(&self.param1)?;
        encoder.write(&self.param2)?;

        write_node_metadata(&self.node_metadata, &mut encoder)?;
        write_static_objects(&self.static_objects, &mut encoder)?;
        write_node_timers(&self.node_timers, &mut encoder)?;

        encoder.finish()
    }

    /// Creates an unloaded map block that only contains CONTENT_IGNORE
    pub fn unloaded() -> Self {
        MapBlock {
            map_format_version: 29,
            flags: 0,
            lighting_complete: 0,
            timestamp: 0xffffffff,
            name_id_mappings: HashMap::from([(0, b"ignore".to_vec())]),
            content_width: 2,
            params_width: 2,
            param0: [0; MAPBLOCK_SIZE],
            param1: [0; MAPBLOCK_SIZE],
            param2: [0; MAPBLOCK_SIZE],
            node_metadata: vec![],
            node_timers: vec![],
            static_objects: vec![],
        }
    }

    /// Gets the content type string from a content ID
    pub fn content_from_id(&self, content_id: u16) -> &[u8] {
        self.name_id_mappings
            .get(&content_id)
            .map(|v| v.as_slice())
            .unwrap_or(CONTENT_UNKNOWN)
    }

    /// Queries the mapblock for a node on the given mapblock-relative coordinates
    pub fn get_node_at(&self, relative_node_pos: Position) -> Node {
        let index = relative_node_pos.as_node_index() as usize % MAPBLOCK_SIZE;
        let param0 = self.content_from_id(self.param0[index]);
        Node {
            param0: std::string::String::from_utf8_lossy(param0).into(),
            param1: self.param1[index],
            param2: self.param2[index],
        }
    }

    /// Gather the content ID associated with this content name, if present
    pub fn get_content_id(&self, content: &[u8]) -> Option<u16> {
        self.name_id_mappings.iter().find(|(_k, v)| v == &content).map(|(&k, _v)| k)
    }

    /// Add a new content string, returning a new content ID
    ///
    /// Panics if there are already ~65k content IDs present
    fn add_content(&mut self, content: Vec<u8>) -> u16 {
        for id in u16::MIN .. u16::MAX {
            if !self.name_id_mappings.contains_key(&id) {
                assert_eq!(self.name_id_mappings.insert(id, content), None);
                return id
            }
        }
        panic!("Did not find a fresh content ID in whole u16 range")
        // Instead of panicking, one could also search for an unused content ID
    }

    /// Return the content ID associated with this content name
    ///
    /// If not present yet, it is created.
    pub fn get_or_create_content_id(&mut self, content: &[u8]) -> u16 {
        self.get_content_id(content)
            .unwrap_or_else(|| self.add_content(content.to_vec()))
    }

    /// Sets the content string of this node
    pub fn set_content(&mut self, relative_node_pos: Position, content_id: u16) {
        let index = relative_node_pos.as_node_index() as usize % MAPBLOCK_SIZE;
        self.param0[index] = content_id
    }

    /// Sets the param1 of this node
    pub fn set_param1(&mut self, relative_node_pos: Position, param1: u8) {
        let index = relative_node_pos.as_node_index() as usize % MAPBLOCK_SIZE;
        self.param1[index] = param1
    }

    /// Sets the param2 of this node
    pub fn set_param2(&mut self, relative_node_pos: Position, param2: u8) {
        let index = relative_node_pos.as_node_index() as usize % MAPBLOCK_SIZE;
        self.param2[index] = param2
    }

    /// Returns an iterator over all content types that appear in name-id-mapping
    ///
    /// Example:
    /// ```
    /// use minetestworld::MapBlock;
    ///
    /// let block = MapBlock::unloaded();
    /// let content_names: Vec<&[u8]> = block.content_names().collect();
    /// assert_eq!(vec![b"ignore"], content_names);
    /// ```
    pub fn content_names(&self) -> impl Iterator<Item=&[u8]> {
        self.name_id_mappings.values().map(Vec::as_slice)
    }
}

// Helper functions to read smaller chunks of binary data

fn read_name_id_mappings(data: &mut impl Read) -> Result<NameIdMappings, MapBlockError> {
    if read_u8(data)? != 0 {
        return Err(MapBlockError::BlobMalformed(
            "name_id_mappings version byte is not zero".into(),
        ));
    }

    let num_name_id_mappings = read_u16_be(data)?;
    let mut name_id_mappings = HashMap::new();
    for _ in 0..num_name_id_mappings {
        let id = read_u16_be(data)?;
        let mut name = vec![0; read_u16_be(data)? as usize];
        data.read_exact(&mut name)?;

        if let Some(old_name) = name_id_mappings.insert(id, name.clone()) {
            return Err(MapBlockError::BlobMalformed(
                format!(
                    "Node ID {id} appears multiple times in name_id_mappings: \"{}\" and \"{}\"",
                    std::string::String::from_utf8_lossy(&old_name),
                    std::string::String::from_utf8_lossy(&name)
                ),
            ));
        }
    }
    Ok(name_id_mappings)
}

fn write_name_id_mappings(mappings: &NameIdMappings, dest: &mut impl Write) -> std::io::Result<()> {
    dest.write(&[0])?; // Version byte
    dest.write(&(mappings.len() as u16).to_be_bytes())?; // TODO handle length greater than 65k
    for (key, value) in mappings {
        dest.write(&key.to_be_bytes())?;
        dest.write(&(value.len() as u16).to_be_bytes())?;
        dest.write(value)?;
    }
    Ok(())
}

fn read_inventory(data: &mut impl Read) -> std::io::Result<Vec<u8>> {
    let mut result = vec![];
    let mut line = vec![];

    for byte in data.bytes() {
    let byte = byte?;
        line.push(byte);
        if byte == 10 {
            result.extend_from_slice(&mut line);
            if line == b"EndInventory\n" {
                return Ok(result)
            }
            line.clear();
        }
    }

    Err(std::io::Error::new(std::io::ErrorKind::UnexpectedEof, "inventory"))
}

fn read_node_metadata(data: &mut impl Read) -> Result<Vec<NodeMetadata>, MapBlockError> {
    let metadata_version = read_u8(data)?;
    if metadata_version == 0 {
        return Ok(vec![]);
    }
    if metadata_version != 2 {
        return Err(MapBlockError::UnsupportedNodeMetadataVersion(metadata_version));
    }
    let metadata_count = read_u16_be(data)?;
    let metadata = Vec::with_capacity(metadata_count as usize);

    for _ in 0..metadata_count {
        let mut metadatum = NodeMetadata {
            position: Position::from_node_index(read_u16_be(data)?),
            vars: Default::default(),
            inventory: vec![],
        };

        let var_count = read_u32_be(data)?;
        for _ in 0..var_count {
            let mut key = vec![0; read_u16_be(data)? as usize];
            data.read_exact(&mut key)?;
            let mut value = vec![0; read_u32_be(data)? as usize];
            data.read_exact(&mut value)?;
            let is_private = read_u8(data)?;
            if is_private > 1 {
                return Err(MapBlockError::BlobMalformed("is_private is not 0 or 1".into()));
            }

            metadatum.vars.push(NodeVar {
                key, value,
                is_private: is_private == 1,
            });
        }
        metadatum.inventory = read_inventory(data)?;
    }

    Ok(metadata)
}

fn write_node_metadata(data: &[NodeMetadata], dest: &mut impl Write) -> std::io::Result<()> {
    if data.is_empty() {
        dest.write(&[0])?;
    } else {
        dest.write(&[2])?;
        dest.write(&(data.len() as u16).to_be_bytes())?; // TODO handle count greater than 65k
        for metadatum in data {
            dest.write(&metadatum.position.as_node_index().to_be_bytes())?;
            for var in &metadatum.vars {
                dest.write(&(var.key.len() as u16).to_be_bytes())?;
                dest.write(&var.key)?;
                dest.write(&(var.value.len() as u32).to_be_bytes())?;
                dest.write(&var.value)?;
                dest.write(&[var.is_private as u8])?;
            }
            dest.write(&metadatum.inventory)?;
        }
    }

    Ok(())
}

fn read_object_pos(data: &mut impl Read) -> std::io::Result<(i32, i32, i32)> {
    let mut bytes = [0; 12];
    data.read_exact(&mut bytes)?;
    Ok((
        // We can safely unwrap here as the slicing numbers are trivially correct
        i32::from_be_bytes(bytes[0..4].try_into().unwrap()),
        i32::from_be_bytes(bytes[4..8].try_into().unwrap()),
        i32::from_be_bytes(bytes[8..12].try_into().unwrap()),
    ))
}

fn read_static_objects(source: &mut impl Read) -> Result<Vec<StaticObject>, MapBlockError> {
    let version = read_u8(source)?;
    if version != 0 {
        return Err(MapBlockError::BlobMalformed(
            format!("static objects version should be 0, is {} ", version)
        ))
    }
    let count = read_u16_be(source)?;
    let mut objects = Vec::with_capacity(count as usize);

    for _ in 0..count {
        let type_id = read_u8(source)?;
        let (x,y,z) = read_object_pos(source)?;
        let data_size = read_u16_be(source)?;
        let mut data = vec![0; data_size as usize];
        source.read_exact(&mut data)?;
        objects.push(StaticObject {
            type_id, x, y, z, data
        })
    }

    Ok(objects)
}

fn write_static_objects(data: &[StaticObject], dest: &mut impl Write) -> std::io::Result<()> {
    dest.write(&[0])?;
    dest.write(&(data.len() as u16).to_be_bytes())?;
    for object in data {
        for i in [object.x, object.y, object.z] {
            dest.write(&i.to_be_bytes())?;
        }
        dest.write(&(object.data.len() as u16).to_be_bytes())?;
        dest.write(&object.data)?;
    }
    Ok(())
}

fn read_timers(data: &mut impl Read) -> Result<Vec<NodeTimer>, MapBlockError> {
    let timer_size = read_u8(data)?;
    if timer_size != 10 {
        return Err(MapBlockError::BlobMalformed(
            format!("timer size should be 10, is {} ", timer_size)
        ))
    }

    let count = read_u16_be(data)?;
    let mut timers = Vec::with_capacity(count as usize);

    for _ in 0..count {
        let position = Position::from_node_index(read_u16_be(data)?);
        let timeout = read_i32_be(data)?;
        let elapsed = read_i32_be(data)?;
        timers.push(NodeTimer {
            position, timeout, elapsed,
        })
    }

    Ok(timers)
}

fn write_node_timers(data: &[NodeTimer], dest: &mut impl Write) -> std::io::Result<()> {
    dest.write(&[10])?; // Data length of node timers

    dest.write(&(data.len() as u16).to_be_bytes())?;
    for timer in data {
        dest.write(&timer.position.as_node_index().to_be_bytes())?;
        dest.write(&timer.timeout.to_be_bytes())?;
        dest.write(&timer.elapsed.to_be_bytes())?;
    }

    Ok(())
}

/// Iterates through the nodes in a mapblock.
///
/// This yields tuples in the form ([world_position][`Position`],
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
    /// This is a tuple consisting of the node and its position in the world.
    type Item = (Position, Node);

    fn next(&mut self) -> Option<Self::Item> {
        let index = self.node_index;
        if index < 4096 {
            self.node_index += 1;
            let pos =
                self.mapblock_position * MAPBLOCK_LENGTH as i16 + Position::from_node_index(index);
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
