//! Functions and datatypes to work with world coordinates

use crate::MAPBLOCK_LENGTH;
use glam::{I16Vec3, IVec3, U16Vec3};
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
///
/// - `x`: "East direction". The direction in which the sun rises.
/// - `y`: "Up" direction
/// - `z`: "North" direction. 90° left from the direction the sun rises.
#[repr(transparent)]
#[derive(Debug, PartialEq, Copy, Clone, Eq, Hash)]
pub struct Position(pub I16Vec3);

impl std::ops::Add for Position {
    type Output = Self;

    fn add(self, rhs: Self) -> Self {
        Self(self.0 + rhs.0)
    }
}

impl std::ops::Sub for Position {
    type Output = Self;

    fn sub(self, rhs: Self) -> Self::Output {
        Self(self.0 - rhs.0)
    }
}

impl std::ops::Mul<i16> for Position {
    type Output = Self;

    fn mul(self, rhs: i16) -> Self {
        Self(self.0 * rhs)
    }
}

impl From<I16Vec3> for Position {
    fn from(value: I16Vec3) -> Self {
        Position(value)
    }
}

impl From<Position> for I16Vec3 {
    fn from(value: Position) -> Self {
        value.0
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
        IVec3::new(
            row.try_get("posx")?,
            row.try_get("posy")?,
            row.try_get("posz")?,
        )
        .try_into()
        .map(Self)
        .map_err(invalid_data_error)
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
        I16Vec3::new(x.into(), y.into(), z.into()).into()
    }

    /// Convert a mapblock database index into coordinates
    pub(crate) fn from_database_key(i: i64) -> Position {
        // i = ........:........:........:....zzzz:zzzzzzzz:yyyyyyyy:yyyyxxxx:xxxxxxxx
        // for each coordinate: left-align within i16; cast to i16; sign extended right-align
        Position::from(I16Vec3::new((i << 4) as i16, (i >> 8) as i16, (i >> 20) as i16) >> 4)
    }

    /// Convert a map block position to an integer
    ///
    /// This integer is used as primary key in the sqlite and redis backends.
    pub(crate) fn as_database_key(self) -> i64 {
        i64::from(self.0.x & 0x0fff)
            | i64::from(self.0.y & 0x0fff) << 12
            | i64::from(self.0.z & 0x0fff) << 24
    }

    /// Convert a nodex index (used in flat 16·16·16 arrays) into a node position
    ///
    /// The node position will be relative to the map block.
    pub(crate) fn from_node_index(node_index: u16) -> Self {
        U16Vec3::new(
            node_index & 0x000f,
            node_index >> 4 & 0x000f,
            node_index >> 8 & 0x000f,
        )
        .as_i16vec3()
        .into()
    }

    /// Convert a MapBlock-relative node position into a flat array index
    pub(crate) fn as_node_index(&self) -> u16 {
        self.0.x as u16 + 16 * self.0.y as u16 + 256 * self.0.z as u16
    }

    /// Return the mapblock position corresponding to this node position
    pub fn mapblock_at(&self) -> Position {
        Position::new(
            div_floor(self.0.x, MAPBLOCK_LENGTH.into()),
            div_floor(self.0.y, MAPBLOCK_LENGTH.into()),
            div_floor(self.0.z, MAPBLOCK_LENGTH.into()),
        )
    }

    /// Split this node position into a mapblock position and a relative node position
    pub fn split_at_block(&self) -> (Position, Position) {
        let blockpos = self.mapblock_at();
        let relative_pos = *self - blockpos * MAPBLOCK_LENGTH as i16;
        (blockpos, relative_pos)
    }
}
