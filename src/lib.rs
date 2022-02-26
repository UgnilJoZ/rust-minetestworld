//! This crate lets you read all chunks of a minetest world,
//! as long as they are already saved in map format version 29.
//!
//! An example that reads all nodes of a specific chunk:
//! ```
//! use minetestworld::{World, Position};
//!
//! use async_std::task;
//!
//! let blockpos = Position {
//!     x: -13,
//!     y: -8,
//!     z: 2,
//! };
//! 
//! task::block_on(async {
//!     let world = World::new("TestWorld");
//!     let mapdata = world.get_map().await.unwrap();
//!     for (pos, node) in mapdata.iter_mapblock_nodes(blockpos).await.unwrap() {
//!         println!("{pos:?}, {node:?}");
//!     }
//! });

extern crate async_std;
#[cfg(feature = "smartstring")]
extern crate smartstring;

pub mod map_block;
pub mod map_data;
pub mod positions;
pub mod world;

pub use map_block::MapBlock;
pub use map_block::Node;
pub use map_data::MapData;
pub use map_data::MapDataError;
pub use positions::Position;
pub use world::World;

pub use map_block::MAPBLOCK_LENGTH;
pub use map_block::MAPBLOCK_SIZE;

#[cfg(test)]
mod tests;
