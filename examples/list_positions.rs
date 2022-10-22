use minetestworld::World;

#[async_std::main]
async fn main() {
    let world = World::new("TestWorld");
    let data = world.get_map_data().await.unwrap();
    for pos in data.all_mapblock_positions().await.unwrap() {
        println!("{pos:?}");
    }
}
