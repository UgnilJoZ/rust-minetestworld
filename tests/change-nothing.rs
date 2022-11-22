use std::error::Error;
mod common;
use futures::TryStreamExt;
use minetestworld::World;

/// Reading and writing a block should be more-or-less a no-op
async fn nop() -> Result<(), Box<dyn Error>> {
    let world = World::open("TestWorld copy");
    let data = world.get_map_data_backend(false).await?;
    let positions: Vec<_> = data.all_mapblock_positions().await.try_collect().await?;
    for pos in positions {
        let block1 = data.get_mapblock(pos).await?;
        data.set_mapblock(pos, &block1).await?;
        let block2 = data.get_mapblock(pos).await?;
        // Test a few attributes on equality
        assert_eq!(block1.flags, block2.flags);
        assert_eq!(block1.param0, block2.param0);
        assert_eq!(block1.param1, block2.param1);
        assert_eq!(block1.param2, block2.param2);
        assert_eq!(block1.lighting_complete, block2.lighting_complete);
        assert_eq!(block1.name_id_mappings, block2.name_id_mappings);
        assert_eq!(block1.timestamp, block2.timestamp);
        assert_eq!(block1.node_timers.len(), block2.node_timers.len());
        assert_eq!(block1.static_objects.len(), block2.static_objects.len());
        assert_eq!(block1.node_metadata.len(), block2.node_metadata.len());
    }
    Ok(())
}

#[async_std::test]
async fn test_nop() -> Result<(), Box<dyn Error>> {
    common::tear_up().await?;
    // No early return here, so that tear down happens in every case
    let result = nop().await;
    let cleanup_result = common::tear_down().await;
    result?;
    cleanup_result?;
    Ok(())
}
