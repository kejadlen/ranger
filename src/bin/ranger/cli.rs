//! Hand-rolled argument parsing on top of [`lexopt`].
//!
//! lexopt only tokenizes the command line — subcommand dispatch, help text,
//! env-var defaults, and "missing required argument" errors are all written
//! out by hand here and in each command module's `parse` function.

use std::ffi::OsString;
use std::path::PathBuf;

use lexopt::{Arg, Parser, ValueExt};

use crate::commands;
use crate::output::ColorChoice;

/// Build-time version set by build.rs (dev) or release workflow.
pub const VERSION: &str = env!("RANGER_VERSION");

const HELP: &str = "\
Personal task tracker

Usage: ranger [OPTIONS] [COMMAND]

Commands:
  backlog  Manage backlogs [alias: b]
  task     Manage tasks [alias: t]
  comment  Manage comments [alias: c]
  tag      Manage tags [alias: g]
  serve    Start the web server

Running with no command shows $RANGER_DEFAULT_BACKLOG, or lists all backlogs.

Options:
      --json          Output as JSON
      --color <WHEN>  When to colorize output [auto|always|never]
      --db <PATH>     Path to database file [env: RANGER_DB]
                      (default: $XDG_DATA_HOME/ranger/ranger.db)
  -h, --help          Print help
  -V, --version       Print version
";

/// A parsed invocation: global flags plus an optional command.
pub struct Cli {
    pub globals: Globals,
    pub command: Option<Command>,
}

pub enum Command {
    Backlog(commands::backlog::BacklogCommands),
    Task(commands::task::TaskCommands),
    Comment(commands::comment::CommentCommands),
    Tag(commands::tag::TagCommands),
    Serve { port: u16, backlog: Option<String> },
}

/// Flags accepted anywhere on the command line, clap's `global = true`.
#[derive(Default)]
pub struct Globals {
    pub json: bool,
    pub color: ColorChoice,
    pub db: Option<PathBuf>,
}

impl Globals {
    /// Handle a global flag or `--help`; anything else is an error. Every
    /// parse loop funnels its unrecognized arguments here.
    pub fn consume(
        &mut self,
        arg: OwnedArg,
        parser: &mut Parser,
        help: &str,
    ) -> Result<(), lexopt::Error> {
        match arg {
            OwnedArg::Short('h') => help_exit(help),
            OwnedArg::Long(l) => match l.as_str() {
                "json" => self.json = true,
                "color" => self.color = parser.value()?.parse()?,
                "db" => self.db = Some(PathBuf::from(parser.value()?)),
                "help" => help_exit(help),
                _ => return Err(lexopt::Error::UnexpectedOption(format!("--{l}"))),
            },
            other => return Err(other.unexpected()),
        }
        Ok(())
    }
}

/// An [`Arg`] that no longer borrows the parser. `Arg::Long` borrows
/// `Parser`'s internal buffer, so an arm that binds the whole `Arg` can't
/// also call `parser.value()`; converting to this owned form releases the
/// borrow so helpers can take `&mut Parser` alongside the argument.
pub enum OwnedArg {
    Short(char),
    Long(String),
    Value(OsString),
}

impl From<Arg<'_>> for OwnedArg {
    fn from(arg: Arg<'_>) -> Self {
        match arg {
            Arg::Short(c) => OwnedArg::Short(c),
            Arg::Long(l) => OwnedArg::Long(l.to_owned()),
            Arg::Value(v) => OwnedArg::Value(v),
        }
    }
}

impl OwnedArg {
    /// Mirror of [`Arg::unexpected`] for the owned form.
    pub fn unexpected(self) -> lexopt::Error {
        match self {
            OwnedArg::Short(c) => lexopt::Error::UnexpectedOption(format!("-{c}")),
            OwnedArg::Long(l) => lexopt::Error::UnexpectedOption(format!("--{l}")),
            OwnedArg::Value(v) => lexopt::Error::UnexpectedArgument(v),
        }
    }
}

/// A usage error lexopt has no variant for (unknown subcommand, missing
/// required argument, ...).
pub fn error(msg: impl Into<String>) -> lexopt::Error {
    lexopt::Error::Custom(msg.into().into())
}

/// Unwrap a required argument or fail with a usage error naming it.
pub fn required<T>(value: Option<T>, name: &str) -> Result<T, lexopt::Error> {
    value.ok_or_else(|| error(format!("missing required argument {name}")))
}

/// The `RANGER_DEFAULT_BACKLOG` fallback used by several arguments
/// (clap's `env = ...` attribute, by hand).
pub fn default_backlog() -> Option<String> {
    std::env::var("RANGER_DEFAULT_BACKLOG").ok()
}

pub fn help_exit(help: &str) -> ! {
    print!("{help}");
    std::process::exit(0);
}

/// Advance to a command group's subcommand (the next positional), consuming
/// any global flags on the way. Prints the group's help and exits if the
/// subcommand is missing.
pub fn subcommand(
    parser: &mut Parser,
    globals: &mut Globals,
    help: &str,
) -> Result<String, lexopt::Error> {
    while let Some(arg) = parser.next()? {
        match arg {
            Arg::Value(v) => return v.string(),
            other => {
                let other = OwnedArg::from(other);
                globals.consume(other, parser, help)?;
            }
        }
    }
    eprint!("{help}");
    std::process::exit(2);
}

/// Consume the rest of the arguments, accepting only global flags. For
/// subcommands that take no arguments of their own.
pub fn drain(parser: &mut Parser, globals: &mut Globals, help: &str) -> Result<(), lexopt::Error> {
    while let Some(arg) = parser.next()? {
        let arg = OwnedArg::from(arg);
        globals.consume(arg, parser, help)?;
    }
    Ok(())
}

pub fn parse(mut parser: Parser) -> Result<Cli, lexopt::Error> {
    let mut globals = Globals::default();
    let mut command = None;
    while let Some(arg) = parser.next()? {
        match arg {
            Arg::Short('V') | Arg::Long("version") => {
                println!("ranger {VERSION}");
                std::process::exit(0);
            }
            Arg::Value(v) if command.is_none() => {
                let name = v.string()?;
                command = Some(match name.as_str() {
                    "backlog" | "b" => {
                        Command::Backlog(commands::backlog::parse(&mut parser, &mut globals)?)
                    }
                    "task" | "t" => {
                        Command::Task(commands::task::parse(&mut parser, &mut globals)?)
                    }
                    "comment" | "c" => {
                        Command::Comment(commands::comment::parse(&mut parser, &mut globals)?)
                    }
                    "tag" | "g" => Command::Tag(commands::tag::parse(&mut parser, &mut globals)?),
                    "serve" => commands::serve::parse(&mut parser, &mut globals)?,
                    _ => return Err(error(format!("unrecognized subcommand '{name}'"))),
                });
            }
            other => {
                let other = OwnedArg::from(other);
                globals.consume(other, &mut parser, HELP)?;
            }
        }
    }
    Ok(Cli { globals, command })
}
