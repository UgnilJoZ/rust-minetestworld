//! Contains functions and datatypes to work with world coordinates

use std::ops::{Add, Rem};

/// Coordinates in the world
///
/// This type is used for addressing either nodes or map blocks.
///
/// In the latter case, the components are divided by the
/// [chunk length](`crate::MAPBLOCK_LENGTH`).
#[derive(Debug, PartialEq, Copy, Clone)]
pub struct Position {
    /// "West direction". The direction in which the sun goes down.
    pub x: i16,
    /// "Up" direction
    pub y: i16,
    /// "North" direction
    pub z: i16,
}

// impl<'r, R> sqlx::FromRow<'r, R> for Position where R: sqlx::Row {
//     fn from_row(row: &'r R) -> Result<Self, sqlx::Error> {
//         Ok(get_integer_as_block(row.get("pos")?))
//     }
// }

impl std::ops::Add for Position {
    type Output = Self;

    fn add(self, other: Self) -> Self {
        Position {
            x: self.x + other.x,
            y: self.y + other.y,
            z: self.z + other.z,
        }
    }
}

/// While there is no modulo operator in rust, we'll use the remainder operator (%) to build one.
pub fn modulo<I>(a: I, b: I) -> I
where
    I: Copy + Add<Output = I> + Rem<Output = I>,
{
    ((a % b + b) % b) as I
}

/// Convert a mapblock database index into coordinates
pub(crate) fn get_integer_as_block(i: i64) -> Position {
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

/// Convert a map block position into a database index, used as primary key
pub(crate) fn get_block_as_integer(p: Position) -> i64 {
    p.x as i64 + p.y as i64 * 4096 + p.z as i64 * 16777216
}

/// Convert a nodex index (used in flat 16·16·16 arrays) into a node position
///
/// The node position will be relative to the map block.
pub(crate) fn mapblock_node_position(node_index: u16) -> Position {
    let x = node_index % 16;
    let i = node_index / 16;
    let y = i % 16;
    let i = node_index / 16;
    let z = i % 16;
    Position {
        x: x as i16,
        y: y as i16,
        z: z as i16,
    }
}

/// Convert a MapBlock-relative node position into a flat array index
pub(crate) fn mapblock_node_index(rel_x: u8, rel_y: u8, rel_z: u8) -> u16 {
    rel_x as u16 + 16 * rel_y as u16 + 256 * rel_z as u16
}
