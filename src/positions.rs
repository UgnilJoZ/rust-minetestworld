//! Functions and datatypes to work with world coordinates

use crate::MAPBLOCK_LENGTH;
use num_integer::div_floor;
#[cfg(feature = "postgres")]
use sqlx::postgres::PgRow;
#[cfg(feature = "sqlite")]
use sqlx::sqlite::SqliteRow;
#[cfg(any(feature = "sqlite", feature = "postgres"))]
use sqlx::{FromRow, Row};
use std::io;
use std::ops::{Add, Rem};

/// A point location within the world
///
/// This type is used for addressing one of the following:
/// * voxels ([nodes](`crate::Node`), node timers, metadata, ...).
/// * [MapBlocks](`crate::MapBlock`). In this case, all three dimensions are divided by the
/// MapBlock [side length](`crate::MAPBLOCK_LENGTH`).
///
/// A voxel position may either be absolute or relative to a mapblock root.
#[derive(Debug, PartialEq, Copy, Clone, Eq, Hash)]
pub struct Position {
    /// "East direction". The direction in which the sun rises.
    pub x: i16,
    /// "Up" direction
    pub y: i16,
    /// "North" direction. 90° left from the direction the sun rises.
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

impl std::ops::Sub for Position {
    type Output = Self;

    fn sub(self, rhs: Self) -> Self::Output {
        Position {
            x: self.x - rhs.x,
            y: self.y - rhs.y,
            z: self.z - rhs.z,
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

fn invalid_data_error<E>(error: E) -> sqlx::Error
where
    E: Into<Box<dyn std::error::Error + Send + Sync>>,
{
    sqlx::Error::Io(io::Error::new(io::ErrorKind::InvalidData, error))
}

#[cfg(feature = "sqlite")]
impl FromRow<'_, SqliteRow> for Position {
    fn from_row(row: &SqliteRow) -> sqlx::Result<Self> {
        Ok(Position::from_database_key(row.try_get("pos")?))
    }
}

#[cfg(feature = "postgres")]
impl FromRow<'_, PgRow> for Position {
    /// Will fail if one of the pos components do not fit in an i16
    fn from_row(row: &PgRow) -> sqlx::Result<Self> {
        let x: i32 = row.try_get("posx")?;
        let y: i32 = row.try_get("posy")?;
        let z: i32 = row.try_get("posz")?;
        Ok(Position {
            x: x.try_into().map_err(invalid_data_error)?,
            y: y.try_into().map_err(invalid_data_error)?,
            z: z.try_into().map_err(invalid_data_error)?,
        })
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

    /// Convert a map block position to an integer
    ///
    /// This integer is used as primary key in the sqlite and redis backends.
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

    /// Return the mapblock position corresponding to this node position
    pub fn mapblock_at(&self) -> Position {
        Position {
            x: div_floor(self.x, MAPBLOCK_LENGTH.into()),
            y: div_floor(self.y, MAPBLOCK_LENGTH.into()),
            z: div_floor(self.z, MAPBLOCK_LENGTH.into()),
        }
    }

    /// Split this node position into a mapblock position and a relative node position
    pub fn split_at_block(&self) -> (Position, Position) {
        let blockpos = self.mapblock_at();
        let relative_pos = *self - blockpos * MAPBLOCK_LENGTH as i16;
        (blockpos, relative_pos)
    }
}
