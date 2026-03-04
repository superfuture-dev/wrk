use std::path::PathBuf;

use chrono::NaiveDate;
use clap::{Args, Parser, Subcommand, ValueEnum};

#[derive(Debug, Parser)]
#[command(name = "wrk", version, about = "Track daily work in markdown logs")]
pub struct LogCli {
    #[arg(long, global = true, value_name = "PATH")]
    pub config: Option<PathBuf>,

    #[arg(long, global = true, value_name = "PATH")]
    pub log_dir: Option<PathBuf>,

    #[arg(short, long, value_name = "PROJECT")]
    pub project: Option<String>,

    #[arg(short = 't', long = "type", value_name = "TYPE")]
    pub kind: Option<String>,

    #[arg(
        value_name = "MESSAGE",
        num_args = 1..,
        trailing_var_arg = true,
        allow_hyphen_values = true
    )]
    pub message: Vec<String>,
}

#[derive(Debug, Parser)]
#[command(
    name = "wrk",
    version,
    about = "Track daily work in markdown logs",
    args_conflicts_with_subcommands = true
)]
pub struct RootHelpCli {
    #[arg(long, global = true, value_name = "PATH")]
    pub config: Option<PathBuf>,

    #[arg(long, global = true, value_name = "PATH")]
    pub log_dir: Option<PathBuf>,

    #[arg(short, long, value_name = "PROJECT")]
    pub project: Option<String>,

    #[arg(short = 't', long = "type", value_name = "TYPE")]
    pub kind: Option<String>,

    #[command(subcommand)]
    pub command: Option<Command>,

    #[arg(
        value_name = "MESSAGE",
        num_args = 1..,
        trailing_var_arg = true,
        allow_hyphen_values = true
    )]
    pub message: Vec<String>,
}

#[derive(Debug, Parser)]
#[command(name = "wrk", version, about = "Track daily work in markdown logs")]
pub struct CommandCli {
    #[arg(long, global = true, value_name = "PATH")]
    pub config: Option<PathBuf>,

    #[arg(long, global = true, value_name = "PATH")]
    pub log_dir: Option<PathBuf>,

    #[command(subcommand)]
    pub command: Command,
}

#[derive(Debug, Subcommand)]
pub enum Command {
    Day(ViewArgs),
    Week(ViewArgs),
    Month(ViewArgs),
    Year(ViewArgs),
    Search(SearchArgs),
    Project(ProjectArgs),
    Edit(EditArgs),
    Amend(AmendArgs),
    Doctor,
    Emoji(EmojiArgs),
}

#[derive(Debug, Clone, Args)]
pub struct ViewArgs {
    #[arg(long, value_name = "YYYY-MM-DD")]
    pub date: Option<NaiveDate>,

    #[arg(long, value_enum)]
    pub sort: Option<SortMode>,

    #[arg(short = 'a', long = "all")]
    pub all: bool,

    #[arg(long)]
    pub shortcodes: bool,
}

#[derive(Debug, Clone, Args)]
pub struct SearchArgs {
    #[arg(value_name = "PATTERN")]
    pub pattern: String,

    #[arg(short = 'a', long = "all")]
    pub all: bool,

    #[arg(long)]
    pub shortcodes: bool,
}

#[derive(Debug, Clone, Args)]
pub struct ProjectArgs {
    #[arg(value_name = "PROJECT")]
    pub project: String,

    #[arg(short = 'a', long = "all")]
    pub all: bool,

    #[arg(long)]
    pub shortcodes: bool,
}

#[derive(Debug, Clone, Args)]
pub struct EditArgs {
    #[arg(long, value_name = "YYYY-MM-DD")]
    pub date: Option<NaiveDate>,
}

#[derive(Debug, Clone, Args)]
pub struct AmendArgs {
    #[arg(short, long, value_name = "PROJECT")]
    pub project: Option<String>,

    #[arg(short = 't', long = "type", value_name = "TYPE")]
    pub kind: Option<String>,

    #[arg(
        value_name = "MESSAGE",
        num_args = 1..,
        trailing_var_arg = true,
        allow_hyphen_values = true
    )]
    pub message: Vec<String>,
}

#[derive(Debug, Clone, Args)]
pub struct EmojiArgs {
    #[arg(value_enum)]
    pub section: EmojiSection,
}

#[derive(Debug, Clone, Copy, Eq, PartialEq, ValueEnum)]
pub enum SortMode {
    Project,
}

#[derive(Debug, Clone, Copy, Eq, PartialEq, ValueEnum)]
pub enum EmojiSection {
    SmileysAndEmotion,
    PeopleAndBody,
    AnimalsAndNature,
    FoodAndDrink,
    TravelAndPlaces,
    Activities,
    Objects,
    Symbols,
    Flags,
}

#[cfg(test)]
mod tests {
    use clap::Parser;

    use super::{CommandCli, LogCli};

    #[test]
    fn log_cli_captures_plain_message() {
        let cli = LogCli::parse_from(["wrk", "Some message"]);
        assert_eq!(cli.message, vec!["Some message"]);
    }

    #[test]
    fn log_cli_captures_message_after_flags() {
        let cli = LogCli::parse_from(["wrk", "-p", "api", "Some message"]);
        assert_eq!(cli.project.as_deref(), Some("api"));
        assert_eq!(cli.message, vec!["Some message"]);
    }

    #[test]
    fn command_cli_parses_subcommand() {
        let cli = CommandCli::parse_from(["wrk", "day"]);
        assert!(matches!(cli.command, super::Command::Day(_)));
    }
}
