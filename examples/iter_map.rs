use async_std::task;
use futures::TryStreamExt;
use minetestworld::World;

fn main() {
    task::block_on(async {
        let world = World::open("TestWorld");
        let data = world.get_map_data_backend(false).await.unwrap();
        let mut positions = data.all_mapblock_positions().await;
        while let Some(pos) = positions.try_next().await.unwrap() {
            let _ = data.get_mapblock(pos).await.unwrap();
        }
    });
}
