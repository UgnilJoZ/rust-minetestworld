use std::error::Error;
mod common;
use minetestworld::{Position, World};

async fn change_voxel() -> Result<(), minetestworld::world::WorldError> {
    let world = World::new("TestWorld copy");
    let mut vm = world.get_voxel_manip(true).await?;
    vm.set_content(Position::new(0i16, 0, 0), b"default:diamond")
        .await?;
    vm.commit().await?;
    std::mem::drop(vm);

    let mut vm = world.get_voxel_manip(true).await?;
    let node = vm.get_node(Position::new(0i16, 0, 0)).await?;
    assert_eq!(node.param0, "default:diamond");
    Ok(())
}

#[async_std::test]
async fn test_change() -> Result<(), Box<dyn Error>> {
    common::tear_up().await?;
    // No early return here, so that tear down happens in every case
    let result = change_voxel().await;
    let cleanup_result = common::tear_down().await;
    result?;
    cleanup_result?;
    Ok(())
}
