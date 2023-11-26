use futures::TryStreamExt;
use minetestworld::{Position, World};

#[async_std::main]
async fn main() {
    let world = World::open("TestWorld");
    let data = world.get_map_data_backend(false).await.unwrap();
    // Collect the positions beforehand, because sqlite
    // does not tolerate concurrent read and write access
    let positions: Vec<_> = data
        .all_mapblock_positions()
        .await
        .try_collect()
        .await
        .unwrap();
    for pos in positions {
        let mut block = data.get_mapblock(pos).await.unwrap();
        for x in 0..8 {
            let content_id = block.get_or_create_content_id(b"default:apple");
            block.set_content(Position::new::<i16>(x, 0, 0), content_id);
        }
        data.set_mapblock(pos, &block).await.unwrap();
    }
}
