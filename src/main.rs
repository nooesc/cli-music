use color_eyre::Result;

mod bridge;

fn main() -> Result<()> {
    color_eyre::install()?;

    let status = bridge::poll_player_status();
    println!("{status:#?}");

    Ok(())
}
