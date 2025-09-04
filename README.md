# minetestworld

This crate lets you read Luanti worlds in a low-level way.

[![Build](https://github.com/UgnilJoZ/rust-minetestworld/actions/workflows/rust.yaml/badge.svg)](https://github.com/UgnilJoZ/rust-minetestworld/actions/workflows/rust.yaml)
[![Crates.io](https://img.shields.io/crates/v/minetestworld.svg)](https://crates.io/crates/minetestworld)
[![Documentation](https://docs.rs/minetestworld/badge.svg)](https://docs.rs/minetestworld/)
[![dependency status](https://deps.rs/crate/minetestworld/0.6.1/status.svg)](https://deps.rs/crate/minetestworld/0.6.1)

# Usage
As this crate returns tokio based futures, you have to specify that along the dependencies:
```toml
[dependencies]
minetestworld = "0.6.1"
tokio = "1"
```

With the **map block API**, you can load chunks from and save them to the map:
  * [`MapData::all_mapblock_positions()`](https://docs.rs/minetestworld/latest/minetestworld/map_data/enum.MapData.html#method.all_mapblock_positions)
  * [`MapData::get_mapblock()`](https://docs.rs/minetestworld/latest/minetestworld/map_data/enum.MapData.html#method.get_mapblock)
  * [`MapData::set_mapblock()`](https://docs.rs/minetestworld/latest/minetestworld/map_data/enum.MapData.html#method.set_mapblock)

For reading and writing **individual voxels** ("nodes") though, the [`VoxelManip`](https://docs.rs/minetestworld/latest/minetestworld/voxel_manip/struct.VoxelManip.html) struct will be more convenient, as [the example](https://github.com/UgnilJoZ/rust-minetestworld/blob/main/examples/modify_map.rs) shows.

## An example

Here is an example that reads all nodes of a random chunk ("map block"):
```toml
[dependencies]
tokio = { version = "1", features = ["full"] }
minetestworld = "0.6.1"
futures = "0.3"
```

```rs
use std::error::Error;
use futures::StreamExt;
use minetestworld::{World, Position};

#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<(), Box<dyn Error>> {
    // Open a world by its path
    let world = World::open("TestWorld");
    let mapdata = world.get_map_data().await?;

    // Take the first mapblock position we can grab
    let mut positions = mapdata.all_mapblock_positions().await;
    let blockpos = positions.next().await.unwrap()?;

    // Iterate all nodes in that mapblock
    for (pos, node) in mapdata.iter_mapblock_nodes(blockpos).await? {
        let param0 = String::from_utf8(node.param0)?;
        println!("{pos:?}, {param0:?}");
    }
    Ok(())
}
```

## Selectable features
The Cargo features `sqlite`, `redis`, `postgres`, and `leveldb` enable the respective map data backend. They are enabled by default and can be selected individually:
```toml
[dependencies]
minetestworld = { version = "0.6.1", default-features = false, features = [ "sqlite" ] }
```

This crate only compiles if at least one backend is enabled, because it becomes pointless without.

To gain TLS support for the `postgres` and `redis` connections, add the `tls-rustls` or the `tls-native-tls` feature.

See [minetest-worldmapper](https://github.com/UgnilJoZ/minetest-worldmapper) for a real-world example.
