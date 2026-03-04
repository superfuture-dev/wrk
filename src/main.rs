mod cli;
mod config;
mod logbook;

use std::ffi::OsString;

use anyhow::Result;
use clap::CommandFactory;
use clap::Parser;

use crate::cli::{Command, CommandCli, LogCli, RootHelpCli};
use crate::config::Config;
use crate::logbook::{
    amend_last_entry, append_entry, build_new_entry, collect_entry_input, collect_period_entries,
    collect_project_entries, format_entries, format_search_results, lint_repository, month_range,
    open_in_editor, print_emoji_section, search_entries, today, work_week_range, year_range,
};

fn main() {
    if let Err(error) = run() {
        eprintln!("error: {error:#}");
        std::process::exit(1);
    }
}

fn run() -> Result<()> {
    let raw_args: Vec<OsString> = std::env::args_os().collect();
    if is_root_help(&raw_args[1..]) {
        RootHelpCli::command().print_long_help()?;
        println!();
        return Ok(());
    }
    if is_root_version(&raw_args[1..]) {
        println!("{}", RootHelpCli::command().render_version());
        return Ok(());
    }

    if should_parse_command(&raw_args[1..]) {
        let cli = CommandCli::parse_from(&raw_args);
        let config = Config::load(cli.config.clone(), cli.log_dir.clone())?;

        match cli.command {
            Command::Day(args) => {
                let date = args.date.unwrap_or_else(today);
                let entries = collect_period_entries(&config.log_dir, date, date)?;
                println!("{}", format_entries(&entries, args.sort, args.all));
            }
            Command::Week(args) => {
                let anchor = args.date.unwrap_or_else(today);
                let (start, end) = work_week_range(anchor);
                let entries = collect_period_entries(&config.log_dir, start, end)?;
                println!("{}", format_entries(&entries, args.sort, args.all));
            }
            Command::Month(args) => {
                let anchor = args.date.unwrap_or_else(today);
                let (start, end) = month_range(anchor)?;
                let entries = collect_period_entries(&config.log_dir, start, end)?;
                println!("{}", format_entries(&entries, args.sort, args.all));
            }
            Command::Year(args) => {
                let anchor = args.date.unwrap_or_else(today);
                let (start, end) = year_range(anchor)?;
                let entries = collect_period_entries(&config.log_dir, start, end)?;
                println!("{}", format_entries(&entries, args.sort, args.all));
            }
            Command::Search(args) => {
                let entries = search_entries(&config.log_dir, &args.pattern)?;
                println!("{}", format_search_results(&entries, args.all));
            }
            Command::Project(args) => {
                let entries = collect_project_entries(&config.log_dir, &args.project)?;
                println!("{}", format_entries(&entries, None, args.all));
            }
            Command::Edit(args) => {
                let date = args.date.unwrap_or_else(today);
                open_in_editor(&config, date)?;
            }
            Command::Amend(args) => {
                let raw = collect_entry_input(&args.message, false)?.ok_or_else(|| {
                    anyhow::anyhow!("provide a replacement message or pipe it via stdin")
                })?;
                let entry =
                    build_new_entry(&config, args.project.as_deref(), args.kind.as_deref(), &raw)?;
                amend_last_entry(&config, today(), entry)?;
            }
            Command::Doctor => {
                let problems = lint_repository(&config.log_dir, &config)?;
                if problems.is_empty() {
                    println!("No problems found.");
                } else {
                    for problem in problems {
                        println!("{}: {}", problem.path.display(), problem.message);
                    }
                }
            }
            Command::Emoji(args) => {
                println!("{}", print_emoji_section(args.section));
            }
        }
    } else {
        let cli = LogCli::parse_from(&raw_args);
        let config = Config::load(cli.config.clone(), cli.log_dir.clone())?;
        handle_default_log(&config, &cli)?;
    }

    Ok(())
}

fn handle_default_log(config: &Config, cli: &LogCli) -> Result<()> {
    let raw = collect_entry_input(&cli.message, true)?;
    let Some(raw) = raw else {
        return Ok(());
    };

    let entry = build_new_entry(config, cli.project.as_deref(), cli.kind.as_deref(), &raw)?;
    append_entry(config, today(), entry)?;
    Ok(())
}

fn should_parse_command(args: &[OsString]) -> bool {
    let mut consume_next = false;
    let mut logging_flag_seen = false;

    for arg in args {
        if consume_next {
            consume_next = false;
            continue;
        }

        let value = arg.to_string_lossy();
        match value.as_ref() {
            "--" => return false,
            "--config" | "--log-dir" => {
                consume_next = true;
            }
            "-p" | "--project" | "-t" | "--type" => {
                logging_flag_seen = true;
                consume_next = true;
            }
            _ if value.starts_with("--config=") || value.starts_with("--log-dir=") => {}
            _ if value.starts_with("--project=") || value.starts_with("--type=") => {
                logging_flag_seen = true;
            }
            _ if value.starts_with('-') => {}
            _ => return !logging_flag_seen && is_known_command(value.as_ref()),
        }
    }

    false
}

fn is_known_command(value: &str) -> bool {
    matches!(
        value,
        "day"
            | "week"
            | "month"
            | "year"
            | "search"
            | "project"
            | "edit"
            | "amend"
            | "doctor"
            | "emoji"
    )
}

fn is_root_help(args: &[OsString]) -> bool {
    args.len() == 1 && matches!(args[0].to_string_lossy().as_ref(), "-h" | "--help")
}

fn is_root_version(args: &[OsString]) -> bool {
    args.len() == 1 && matches!(args[0].to_string_lossy().as_ref(), "-V" | "--version")
}

#[cfg(test)]
mod tests {
    use std::ffi::OsString;

    use super::{is_root_help, is_root_version, should_parse_command};

    #[test]
    fn parses_known_subcommands_before_log_mode() {
        assert!(should_parse_command(&[OsString::from("day")]));
        assert!(should_parse_command(&[
            OsString::from("--log-dir"),
            OsString::from("/tmp/logs"),
            OsString::from("week"),
        ]));
    }

    #[test]
    fn logging_flags_force_default_log_mode() {
        assert!(!should_parse_command(&[
            OsString::from("-p"),
            OsString::from("api"),
            OsString::from("day"),
        ]));
        assert!(!should_parse_command(&[
            OsString::from("--"),
            OsString::from("day")
        ]));
    }

    #[test]
    fn detects_root_help() {
        assert!(is_root_help(&[OsString::from("--help")]));
        assert!(!is_root_help(&[
            OsString::from("day"),
            OsString::from("--help")
        ]));
    }

    #[test]
    fn detects_root_version() {
        assert!(is_root_version(&[OsString::from("--version")]));
        assert!(!is_root_version(&[
            OsString::from("day"),
            OsString::from("--version")
        ]));
    }
}
