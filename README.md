# minetestworld

This crate lets you read minetest worlds in a low-level way.

[![Build](https://github.com/UgnilJoZ/rust-minetestworld/actions/workflows/rust.yaml/badge.svg)](https://github.com/UgnilJoZ/rust-minetestworld/actions/workflows/rust.yaml)
[![Crates.io](https://img.shields.io/crates/v/minetestworld.svg)](https://crates.io/crates/minetestworld)
[![Documentation](https://docs.rs/minetestworld/badge.svg)](https://docs.rs/minetestworld/)
[![dependency status](https://deps.rs/crate/minetestworld/0.5.4/status.svg)](https://deps.rs/crate/minetestworld/0.5.4)

# Usage
As this crate returns tokio based futures, you have to specify that along the dependencies:
```toml
[dependencies]
minetestworld = "0.5.4"
tokio = "1"
```

## An example

Here is an example that reads all nodes of a specific map block:
```toml
[dependencies]
tokio = { version = "1", features = ["full"] }
minetestworld = "0.5.4"
futures = "0.3"
```

```rs
use std::error::Error;
use futures::StreamExt;
use minetestworld::{World, Position};

#[tokio::main(flavor = "current_thread")]
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

## Selectable features
The Cargo features `sqlite`, `redis`, and `postgres` enable the respective map data backend. They are enabled by default and can be selected individually:
```toml
[dependencies]
minetestworld = { version = "0.5.3", default-features = false, features = [ "sqlite" ] }
```

This crate only compiles if at least one backend is enabled, because it becomes pointless without.

To gain TLS support for the `postgres` connection, add the `tls-rustls` or the `tls-native-tls` feature.

See [minetest-worldmapper](https://github.com/UgnilJoZ/minetest-worldmapper) for a real-world example.
