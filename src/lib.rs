//! This crate lets you read all chunks of a minetest world,
//! as long as they are already saved in map format version 29.
//!
//! ## Terminology
//! ### Node
//! [Nodes](`Node`) are the single voxels that the world data consist of. It has three properties:
//! 1. A content type string (like `air` or `default:dirt`)
//! 2. Flags to determine lighting rendering
//! 3. Additional data that can be interpreted based on the content type (e.g. flow information for liquids)
//!
//! This term might originate in the Irrlicht engine.
//!
//! ### MapBlock
//! The world data is divided into chunks that are called [map blocks](`MapBlock`).
//! A map block contains 16·16·16 nodes as well as objects and metadata.
//!
//! It is addressed by a [`Position`] that is divided by [`MAPBLOCK_LENGTH`] in each component.
//!
//! ## Example usage
//!
//! An example that reads all nodes of a specific map block:
//! ```
//! use minetestworld::{World, Position};
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
//!     let mapdata = world.get_map_data().await.unwrap();
//!     for (pos, node) in mapdata.iter_mapblock_nodes(blockpos).await.unwrap() {
//!         println!("{pos:?}, {node:?}");
//!     }
//! });
//! ```
#![warn(missing_docs)]
#![forbid(unsafe_code)]

extern crate async_std;
#[cfg(feature = "smartstring")]
extern crate smartstring;

pub mod map_block;
pub mod map_data;
pub mod positions;
pub mod voxel_manip;
pub mod world;

pub use map_block::MapBlock;
pub use map_block::Node;
pub use map_data::MapData;
pub use map_data::MapDataError;
pub use positions::Position;
pub use voxel_manip::VoxelManip;
pub use world::World;
pub use world::WorldError as Error;

pub use map_block::MAPBLOCK_LENGTH;
pub use map_block::MAPBLOCK_SIZE;

#[cfg(test)]
mod tests;
