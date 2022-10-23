use minetestworld::{Position, World};

#[async_std::main]
async fn main() {
    let world = World::new("TestWorld");
    let data = world.get_map_data_backend(false).await.unwrap();
    for pos in data.all_mapblock_positions().await.unwrap() {
        let mut block = data.get_mapblock(pos).await.unwrap();
        for x in 0..8 {
            let content_id = block.get_or_create_content_id(b"default:apple");
            block.set_content(Position { x, y: 0, z: 0 }, content_id);
        }
        data.set_mapblock(pos, &block).await.unwrap();
    }
}
