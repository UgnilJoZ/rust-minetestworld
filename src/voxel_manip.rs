//! Contains a type to more high-level world reading and writing

use std::collections::hash_map::Entry;
use std::collections::HashMap;

use crate::{MapBlock, MapData, MapDataError, Node, Position};
type Result<T> = std::result::Result<T, MapDataError>;

struct CacheEntry {
    mapblock: MapBlock,
    tainted: bool,
}

/// In-memory world data cache that allows easy handling of single nodes.
///
/// It is an abstraction on top of the MapBlocks the world data consists of.
/// It allows fast reading from and writing to the world.
///
/// All changes to the world have to be committed via [`VoxelManip::commit`].
/// Before this, they are only present in VoxelManip's local cache and lost after drop.
///
/// ⚠️ You want to do a world backup before modifying the map data.
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
                let mapblock = match self.map.get_mapblock(mapblock_pos).await {
                    Ok(mapblock) => Ok(mapblock),
                    // If not in the database, create unloaded mapblock
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

    /// Get a reference to the mapblock at the given block position
    ///
    /// If there is no mapblock at this world position,
    /// a new [unloaded](`MapBlock::unloaded`) mapblock is returned.
    pub async fn get_mapblock(&mut self, mapblock_pos: Position) -> Result<&MapBlock> {
        Ok(&self.get_entry(mapblock_pos).await?.mapblock)
    }

    /// Get the node at the given world position
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
        let entry = &mut self.get_entry(blockpos).await?;
        op(&mut entry.mapblock);
        entry.tainted = true;
        Ok(())
    }

    /// Set a voxel in VoxelManip's cache
    ///
    /// ⚠️ The change will be present locally only. To modify the map,
    /// the change has to be written back via [`VoxelManip::commit`].
    pub async fn set_node(&mut self, node_pos: Position, node: Node) -> Result<()> {
        let (blockpos, nodepos) = node_pos.split_at_block();
        self.modify_mapblock(blockpos, |mapblock| {
            let content_id = mapblock.get_or_create_content_id(&node.param0);
            mapblock.set_content(nodepos, content_id);
            mapblock.set_param1(nodepos, node.param1);
            mapblock.set_param2(nodepos, node.param2);
        })
        .await
    }

    /// Sets the content string at this world position
    ///
    /// `content` has to be the unique [itemstring](https://wiki.minetest.net/Itemstrings).
    /// The use of aliases is not possible, because it would require a Lua runtime
    /// loading all mods.
    ///
    /// ```ignore
    /// vm.set_content(Position::new(8,9,10), b"default:stone").await?;
    /// ```
    ///
    /// ⚠️ Until the change is [commited](`VoxelManip::commit`),
    /// the node will only be changed in the cache.
    pub async fn set_content(&mut self, node_pos: Position, content: &[u8]) -> Result<()> {
        let (blockpos, nodepos) = node_pos.split_at_block();
        self.modify_mapblock(blockpos, |mapblock| {
            let content_id = mapblock.get_or_create_content_id(content);
            mapblock.set_content(nodepos, content_id);
        })
        .await
    }

    /// Sets the lighting parameter at this world position
    ///
    /// ⚠️ Until the change is [commited](`VoxelManip::commit`),
    /// the node will only be changed in the cache.
    pub async fn set_param1(&mut self, node_pos: Position, param1: u8) -> Result<()> {
        let (blockpos, nodepos) = node_pos.split_at_block();
        self.modify_mapblock(blockpos, |mapblock| {
            mapblock.set_param1(nodepos, param1);
        })
        .await
    }

    /// Sets the param2 of the node at this world position
    ///
    /// ⚠️ Until the change is [commited](`VoxelManip::commit`),
    /// the node will only be changed in the cache.
    pub async fn set_param2(&mut self, node_pos: Position, param2: u8) -> Result<()> {
        let (blockpos, nodepos) = node_pos.split_at_block();
        self.modify_mapblock(blockpos, |mapblock| {
            mapblock.set_param2(nodepos, param2);
        })
        .await
    }

    /// Returns true if this world position is cached
    pub fn is_in_cache(&self, node_pos: Position) -> bool {
        let blockpos = node_pos.mapblock_at();
        self.mapblock_cache.contains_key(&blockpos)
    }

    /// Ensures that this world position is in the cache
    pub async fn visit(&mut self, node_pos: Position) -> Result<()> {
        let blockpos = node_pos.mapblock_at();
        self.get_entry(blockpos).await?;
        Ok(())
    }

    /// Apply all changes made to the map
    ///
    /// Without this, all changes made with [`VoxelManip::set_node`], [`VoxelManip::set_content`],
    /// [`VoxelManip::set_param1`], and [`VoxelManip::set_param2`] are lost when this
    /// instance is dropped.
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
