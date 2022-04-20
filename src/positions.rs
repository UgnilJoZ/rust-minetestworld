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
    /// "East direction". The direction in which the sun rises.
    pub x: i16,
    /// "Up" direction
    pub y: i16,
    /// "North" direction
    pub z: i16,
}

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

impl std::ops::Mul<i16> for Position {
    type Output = Self;

    fn mul(self, rhs: i16) -> Self {
        Position {
            x: self.x * rhs,
            y: self.y * rhs,
            z: self.z * rhs,
        }
    }
}

/// While there is no modulo operator in rust, we'll use the remainder operator (%) to build one.
pub fn modulo<I>(a: I, b: I) -> I
where
    I: Copy + Add<Output = I> + Rem<Output = I>,
{
    (a % b + b) % b
}

impl Position {
    /// Create a new position value from its components
    pub fn new<I: Into<i16>>(x: I, y: I, z: I) -> Self {
        Position {
            x: x.into(),
            y: y.into(),
            z: z.into(),
        }
    }

    /// Convert a mapblock database index into coordinates
    pub(crate) fn from_database_key(i: i64) -> Position {
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
    pub(crate) fn as_database_key(&self) -> i64 {
        self.x as i64 + self.y as i64 * 4096 + self.z as i64 * 16777216
    }

    /// Convert a nodex index (used in flat 16·16·16 arrays) into a node position
    ///
    /// The node position will be relative to the map block.
    pub(crate) fn from_node_index(node_index: u16) -> Position {
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
    pub(crate) fn as_node_index(&self) -> u16 {
        self.x as u16 + 16 * self.y as u16 + 256 * self.z as u16
    }
}
