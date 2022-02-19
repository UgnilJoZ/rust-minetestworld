#[derive(Debug, PartialEq, Copy, Clone)]
pub struct Position {
    pub x: i16,
    pub y: i16,
    pub z: i16,
}

// While there is no modulo operator in rust, we'll use the remainder operator (%) to build one.
pub(crate) fn modulo(a: i64, b: i64) -> i64 {
    ((a % b + b) % b) as i64
}

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

pub(crate) fn get_block_as_integer(p: Position) -> i64 {
    p.x as i64 + p.y as i64 * 4096 + p.z as i64 * 16777216
}
