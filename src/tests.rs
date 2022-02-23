use crate::positions::{get_integer_as_block, mapblock_node_position, Position};
use crate::MapBlock;
use crate::MapData;

#[async_std::test]
async fn db_exists() {
    MapData::from_sqlite_file("TestWorld/map.sqlite").await.unwrap();
}

#[async_std::test]
async fn can_query() {
    let mapdata = MapData::from_sqlite_file("TestWorld/map.sqlite").await.unwrap();
    assert_eq!(mapdata.all_mapblock_positions().await.unwrap().len(), 5923);
    let block = mapdata
        .get_block_data(Position {
            x: -13,
            y: -8,
            z: 2,
        })
        .await
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
    MapBlock::from_data(std::fs::File::open("TestWorld/testmapblock").unwrap()).unwrap();
}

#[async_std::test]
async fn can_parse_all_mapblocks() {
    let mapdata = MapData::from_sqlite_file("TestWorld/map.sqlite").await.unwrap();
    let positions: Vec<_> = mapdata.all_mapblock_positions().await.unwrap();
    let blocks: Vec<_> = futures::future::join_all(
        positions
            .iter()
            .map(|pos| mapdata.get_mapblock(pos.clone())),
    )
    .await;
    let succeeded = blocks.iter().filter(|b| b.is_ok()).count();
    let failed = blocks.iter().filter(|b| b.is_err()).count();
    eprintln!("Succeeded parsed blocks: {succeeded}\nFailed blocks: {failed}");
    assert_eq!(failed, 0);
}

#[async_std::test]
async fn count_nodes() {
    let mapdata = MapData::from_sqlite_file("TestWorld/map.sqlite").await.unwrap();
    let count = mapdata
        .iter_mapblock_nodes(Position {
            x: -13,
            y: -8,
            z: 2,
        })
        .await
        .unwrap()
        .count();
    assert_eq!(count, 4096);
}

#[test]
fn node_index() {
    assert_eq!(mapblock_node_position(0), Position { x: 0, y: 0, z: 0 });
    assert_eq!(
        mapblock_node_position(4095),
        Position {
            x: 15,
            y: 15,
            z: 15,
        }
    )
}
