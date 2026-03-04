use clap::Subcommand;
use ranger_lib::db::SqlitePool;
use ranger_lib::ops;

use crate::output;

#[derive(Subcommand)]
pub enum TagCommands {
    /// List all tags
    List,
}

pub async fn run(pool: &SqlitePool, command: TagCommands, json: bool) -> anyhow::Result<()> {
    match command {
        TagCommands::List => {
            let tags = ops::tag::list(pool).await?;
            output::print_list(&tags, json, |t| {
                println!("{}", t.name);
            });
        }
    }
    Ok(())
}
