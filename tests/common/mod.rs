use tokio::fs;

pub async fn tear_up() -> std::io::Result<()> {
    fs::create_dir("TestWorld copy").await?;
    fs::copy("TestWorld/world.mt", "TestWorld copy/world.mt").await?;
    fs::copy("TestWorld/map.sqlite", "TestWorld copy/map.sqlite").await?;
    Ok(())
}

pub async fn tear_down() -> std::io::Result<()> {
    fs::remove_dir_all("TestWorld copy").await
}
