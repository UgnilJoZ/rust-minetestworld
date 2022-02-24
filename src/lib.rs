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
