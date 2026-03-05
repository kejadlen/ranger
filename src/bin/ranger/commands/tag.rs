use clap::Subcommand;
use color_eyre::eyre::Result;
use ranger::db::SqlitePool;
use ranger::ops;

use crate::output;

#[derive(Subcommand)]
pub enum TagCommands {
    /// List all tags
    List,
}

pub async fn run(pool: &SqlitePool, command: TagCommands, json: bool) -> Result<()> {
    let mut conn = pool.acquire().await?;

    match command {
        TagCommands::List => {
            let tags = ops::tag::list(&mut conn).await?;
            output::print_list(&tags, json, |t| {
                println!("{}", t.name);
            });
        }
    }
    Ok(())
}
