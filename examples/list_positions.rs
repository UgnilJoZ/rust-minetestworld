use futures::TryStreamExt;
use minetestworld::World;

#[async_std::main]
async fn main() {
    let world = World::new("TestWorld");
    let data = world.get_map_data().await.unwrap();
    let mut positions = data.all_mapblock_positions().await;
    while let Some(pos) = positions.try_next().await.unwrap() {
        println!("{pos:?}");
    }
}
