//! Contains a type to more high-level world reading and writing

use std::collections::hash_map::Entry;
use std::collections::HashMap;

use crate::{MapBlock, MapData, MapDataError, Node, Position};
type Result<T> = std::result::Result<T, MapDataError>;

struct CacheEntry {
    mapblock: MapBlock,
    tainted: bool,
}

/// In-memory world data cache that does not stick to the mapblock abstraction
///
/// It allows fast reading from and writing to the world. In the latter case,
/// all changes made have to be committed to the world after
/// writing via [`VoxelManip::commit`].
///
/// ⚠️ Believe me, you want to do a world backup before modifying the map data.
pub struct VoxelManip {
    map: MapData,
    mapblock_cache: HashMap<Position, CacheEntry>,
}

impl VoxelManip {
    /// Create a new VoxelManip from a handle to a map data backend
    pub fn new(map: MapData) -> Self {
        VoxelManip {
            map,
            mapblock_cache: HashMap::new(),
        }
    }

    /// Return a cache entry containing the given mapblock
    async fn get_entry(&mut self, mapblock_pos: Position) -> Result<&mut CacheEntry> {
        match self.mapblock_cache.entry(mapblock_pos) {
            Entry::Occupied(e) => Ok(e.into_mut()),
            Entry::Vacant(e) => {
                // If not in the database, create unloaded mapblock
                let mapblock = match self.map.get_mapblock(mapblock_pos).await {
                    Ok(mapblock) => Ok(mapblock),
                    Err(MapDataError::MapBlockNonexistent(_)) => Ok(MapBlock::unloaded()),
                    Err(e) => Err(e),
                }?;
                Ok(e.insert(CacheEntry {
                    mapblock,
                    tainted: false,
                }))
            }
        }
    }

    /// Get a reference to the mapblock at this block position
    pub async fn get_mapblock(&mut self, mapblock_pos: Position) -> Result<&MapBlock> {
        Ok(&self.get_entry(mapblock_pos).await?.mapblock)
    }

    /// Get the node at this world position
    pub async fn get_node(&mut self, node_pos: Position) -> Result<Node> {
        let (blockpos, nodepos) = node_pos.split_at_block();
        Ok(self.get_mapblock(blockpos).await?.get_node_at(nodepos))
    }

    /// Do something with the mapblock at `blockpos` and mark it as modified
    async fn modify_mapblock(
        &mut self,
        blockpos: Position,
        op: impl FnOnce(&mut MapBlock),
    ) -> Result<()> {
        let mut entry = &mut self.get_entry(blockpos).await?;
        op(&mut entry.mapblock);
        entry.tainted = true;
        Ok(())
    }

    /// Set a voxel in the VoxelManip's cache
    pub async fn set_node(&mut self, node_pos: Position, node: Node) -> Result<()> {
        let (blockpos, nodepos) = node_pos.split_at_block();
        self.modify_mapblock(blockpos, |mapblock| {
            let content_id = mapblock.get_or_create_content_id(node.param0.as_bytes());
            mapblock.set_content(nodepos, content_id);
            mapblock.set_param1(nodepos, node.param1);
            mapblock.set_param2(nodepos, node.param2);
        })
        .await
    }

    /// Sets the content string at this world position
    ///
    /// ```ignore
    /// vm.set_content(Position::new(8,9,10), b"default:stone").await?;
    /// ```
    pub async fn set_content(&mut self, node_pos: Position, content: &[u8]) -> Result<()> {
        let (blockpos, nodepos) = node_pos.split_at_block();
        self.modify_mapblock(blockpos, |mapblock| {
            let content_id = mapblock.get_or_create_content_id(content);
            mapblock.set_content(nodepos, content_id);
        })
        .await
    }

    /// Sets the lighting parameter at this world position
    pub async fn set_param1(&mut self, node_pos: Position, param1: u8) -> Result<()> {
        let (blockpos, nodepos) = node_pos.split_at_block();
        self.modify_mapblock(blockpos, |mapblock| {
            mapblock.set_param1(nodepos, param1);
        })
        .await
    }

    /// Sets the param2 of the node at this world position
    pub async fn set_param2(&mut self, node_pos: Position, param2: u8) -> Result<()> {
        let (blockpos, nodepos) = node_pos.split_at_block();
        self.modify_mapblock(blockpos, |mapblock| {
            mapblock.set_param2(nodepos, param2);
        })
        .await
    }

    /// Returns true if the mapblock containing this world position is cached
    pub fn is_in_cache(&self, node_pos: Position) -> bool {
        let blockpos = node_pos.mapblock_at();
        self.mapblock_cache.contains_key(&blockpos)
    }

    /// Ensures that the mapblock containing this world position is in the cache
    pub async fn visit(&mut self, node_pos: Position) -> Result<()> {
        let blockpos = node_pos.mapblock_at();
        self.get_entry(blockpos).await?;
        Ok(())
    }

    /// Apply changes made to the map
    pub async fn commit(&mut self) -> Result<()> {
        // Write modified mapblocks back into the map data
        for (&pos, cache_entry) in self.mapblock_cache.iter_mut() {
            if cache_entry.tainted {
                self.map.set_mapblock(pos, &cache_entry.mapblock).await?;
                cache_entry.tainted = false;
            }
        }

        Ok(())
    }
}
