use clap::Subcommand;

const SKILL_GUIDE: &str = include_str!("../skill.md");

#[derive(Subcommand)]
pub enum InternalCommands {
    /// Print the agent skill guide for this version of ranger
    Skill,
}

pub fn run(command: InternalCommands) {
    match command {
        InternalCommands::Skill => print!("{SKILL_GUIDE}"),
    }
}
