use minetestworld::{Position, World};

#[async_std::main]
async fn main() {
    let world = World::open("TestWorld");
    let mut vm = world.get_voxel_manip(true).await.unwrap();
    for y in 10..20 {
        vm.set_content(Position::new::<i16>(0, y, 0), b"default:diamondblock")
            .await
            .unwrap();
    }
    vm.commit().await.unwrap();
}
