# minetestworld

This crate lets you read minetest worlds in a low-level way.

[![Build](https://github.com/UgnilJoZ/rust-minetestworld/actions/workflows/rust.yaml/badge.svg)](https://github.com/UgnilJoZ/rust-minetestworld/actions/workflows/rust.yaml)
[![Crates.io](https://img.shields.io/crates/v/minetestworld.svg)](https://crates.io/crates/minetestworld)
[![Documentation](https://docs.rs/minetestworld/badge.svg)](https://docs.rs/minetestworld/)
[![dependency status](https://deps.rs/crate/minetestworld/0.5.2/status.svg)](https://deps.rs/crate/minetestworld/0.5.2)

# Usage
As this crate returns async-std based futures, you have to specify that along the dependencies:
```toml
[dependencies]
minetestworld = "0.5.2"
async-std = "1"
```

## An example

Here is an example that reads all nodes of a specific map block:
```toml
[dependencies]
async-std = { version = "1", features = [ "attributes" ] }
minetestworld = "0.5.2"
```

```rs
use std::error::Error;
use async_std::task;
use async_std::stream::StreamExt;
use minetestworld::{World, Position};

#[async_std::main]
async fn main() -> Result<(), Box<dyn Error>> {
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

## Selectable backends
The Cargo features `sqlite`, `redis`, and `postgres` enable the respective map data backend. They are enabled by default and can be selected individually:
```toml
[dependencies]
minetestworld = { version = "0.5.2", default-features = false, features = [ "sqlite" ] }
```

This crate only compiles if at least one backend is enabled, because it becomes pointless without.

See [minetest-worldmapper](https://github.com/UgnilJoZ/minetest-worldmapper) for a real-world example.
