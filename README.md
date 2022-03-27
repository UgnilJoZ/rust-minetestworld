# minetestworld

This crate lets you read minetest worlds in a low-level way.

[![Build](https://github.com/UgnilJoZ/rust-minetestworld/actions/workflows/rust.yaml/badge.svg)](https://github.com/UgnilJoZ/rust-minetestworld/actions/workflows/rust.yaml)
[![Crates.io](https://img.shields.io/crates/v/minetestworld.svg)](https://crates.io/crates/minetestworld)
[![Documentation](https://docs.rs/minetestworld/badge.svg)](https://docs.rs/minetestworld/latest/minetestworld/)
[![dependency status](https://deps.rs/crate/minetestworld/0.3.0/status.svg)](https://deps.rs/crate/minetestworld/0.3.0)

# Usage
As this crate returns async-std based futures, you have to specify that along the dependencies:
```toml
[dependencies]
minetestworld = "0.3"
async-std = "1"
```

Here is an example that reads all nodes of a specific map block:
```rs
use minetestworld::{World, Position};
use async_std::task;

fn main() {
    let blockpos = Position {
        x: -13,
        y: -8,
        z: 2,
    };
    task::block_on(async {
        let world = World::new("TestWorld");
        let mapdata = world.get_map_data().await.unwrap();
        for (pos, node) in mapdata.iter_mapblock_nodes(blockpos).await.unwrap() {
            println!("{pos:?}, {node:?}");
        }
    });
}
```

See <https://github.com/UgnilJoZ/minetest-worldmapper> for a real-world example.
