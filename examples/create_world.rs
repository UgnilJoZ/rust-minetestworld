use minetestworld::{Node, Position, World};

#[async_std::main]
async fn main() {
    let world = World::create_sqlite("NewWorld").await.unwrap();
    let mut vm = world.get_voxel_manip(true).await.unwrap();
    for y in -99..100 {
        for x in -100..100 {
            for z in -100..100 {
                let pos = Position { x, y, z };
                let content: &[u8] = if y > 0 { b"air" } else { b"default:wood" };
                vm.set_node(
                    pos,
                    Node {
                        param0: content.to_vec(),
                        param1: 255,
                        param2: 0,
                    },
                )
                .await
                .unwrap();
            }
        }
    }
    vm.commit().await.unwrap();
}
