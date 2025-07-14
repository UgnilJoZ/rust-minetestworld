use futures::TryStreamExt;
use minetestworld::World;

#[tokio::main(flavor = "current_thread")]
async fn main() {
    let world = World::open("TestWorld");
    let data = world.get_map_data().await.unwrap();
    let mut positions = data.all_mapblock_positions().await;
    while let Some(pos) = positions.try_next().await.unwrap() {
        println!("{pos:?}");
    }
}
