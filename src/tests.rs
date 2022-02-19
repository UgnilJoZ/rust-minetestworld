use positions::{get_integer_as_block, Position};
use rusqlite::Connection;
use rusqlite::OpenFlags;
use MapBlock;
use MapData;
#[test]
fn db_exists() {
    Connection::open_with_flags("test.sqlite", OpenFlags::SQLITE_OPEN_READ_ONLY).unwrap();
}

#[test]
fn can_query() {
    let mapdata = MapData::from_sqlite_file("test.sqlite").unwrap();
    assert_eq!(mapdata.all_mapblock_positions().unwrap().len(), 5923);
    let block = mapdata
        .get_block_data(Position {
            x: -13,
            y: -8,
            z: 2,
        })
        .unwrap();
    assert_eq!(block.len(), 40);
}

#[test]
fn simple_math() {
    assert_eq!(
        get_integer_as_block(134270984),
        Position { x: 8, y: 13, z: 8 }
    );
    assert_eq!(
        get_integer_as_block(-184549374),
        Position { x: 2, y: 0, z: -11 }
    );
}

#[test]
fn can_parse_mapblock() {
    MapBlock::from_data(std::fs::File::open("testmapblock").unwrap()).unwrap();
}

#[test]
fn can_parse_all_mapblocks() {
    let mapdata = MapData::from_sqlite_file("test.sqlite").unwrap();
    let blocks: Vec<_> = mapdata
        .all_mapblock_positions()
        .unwrap()
        .into_iter()
        .map(|pos| mapdata.get_mapblock(pos))
        .collect();
    let succeeded = blocks.iter().filter(|b| b.is_ok()).count();
    let failed = blocks.iter().filter(|b| b.is_err()).count();
    eprintln!("Succeeded parsed blocks: {succeeded}\nFailed blocks: {failed}");
    assert_eq!(failed, 0);
}
