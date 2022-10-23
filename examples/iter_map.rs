use async_std::task;
use minetestworld::World;

fn main() {
    task::block_on(async {
        let world = World::new("TestWorld");
        let data = world.get_map_data_backend(false).await.unwrap();
        for pos in data.all_mapblock_positions().await.unwrap() {
            let _ = data.get_mapblock(pos).await.unwrap();
        }
    });
}
