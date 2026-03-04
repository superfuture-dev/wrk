use std::cmp::Ordering;
use std::collections::BTreeMap;
use std::fs;
use std::io::{self, IsTerminal, Read};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::OnceLock;

use anyhow::{Context, Result, anyhow, bail};
use chrono::{Datelike, Duration, Local, NaiveDate, NaiveTime, Timelike};
use regex::{Captures, Regex};
use walkdir::WalkDir;

use crate::cli::{EmojiSection, SortMode};
use crate::config::Config;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Entry {
    pub date: NaiveDate,
    pub timestamp: NaiveTime,
    pub project: Option<String>,
    pub kind: String,
    pub summary: String,
    pub details: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct DayDocument {
    pub date: NaiveDate,
    pub entries: Vec<Entry>,
    pub notes: String,
}

#[derive(Debug, Clone)]
pub struct NewEntry {
    pub project: Option<String>,
    pub kind: String,
    pub summary: String,
    pub details: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct LintProblem {
    pub path: PathBuf,
    pub message: String,
}

pub fn today() -> NaiveDate {
    Local::now().date_naive()
}

pub fn collect_entry_input(message_parts: &[String], interactive: bool) -> Result<Option<String>> {
    if !message_parts.is_empty() {
        return Ok(Some(message_parts.join(" ")));
    }

    let stdin_piped = !io::stdin().is_terminal();
    if stdin_piped {
        let mut buffer = String::new();
        io::stdin()
            .read_to_string(&mut buffer)
            .context("failed to read stdin")?;
        let trimmed = buffer.trim_end_matches(['\n', '\r']);
        if trimmed.is_empty() {
            return Ok(None);
        }
        return Ok(Some(trimmed.to_owned()));
    }

    if interactive {
        collect_interactive_entry()
    } else {
        Ok(None)
    }
}

pub fn build_new_entry(
    config: &Config,
    project: Option<&str>,
    kind: Option<&str>,
    raw: &str,
) -> Result<NewEntry> {
    let project = config.resolve_project(project)?;
    let kind = config.resolve_kind(kind)?;
    let normalized = raw.replace("\r\n", "\n");
    let lines: Vec<String> = normalized.lines().map(expand_emoji_shortcodes).collect();

    let Some(summary) = lines.first().map(|line| line.trim().to_owned()) else {
        bail!("entry cannot be empty");
    };
    if summary.is_empty() {
        bail!("entry cannot be empty");
    }

    let details = lines
        .into_iter()
        .skip(1)
        .map(|line| line.trim_end().to_owned())
        .collect();

    Ok(NewEntry {
        project,
        kind,
        summary,
        details,
    })
}

pub fn append_entry(config: &Config, date: NaiveDate, entry: NewEntry) -> Result<PathBuf> {
    let mut doc = load_day_document(&config.log_dir, date)?;
    doc.entries.push(Entry {
        date,
        timestamp: Local::now()
            .time()
            .with_second(0)
            .unwrap()
            .with_nanosecond(0)
            .unwrap(),
        project: entry.project,
        kind: entry.kind,
        summary: entry.summary,
        details: entry.details,
    });
    save_day_document(config, &doc)
}

pub fn amend_last_entry(config: &Config, date: NaiveDate, entry: NewEntry) -> Result<PathBuf> {
    let mut doc = load_day_document(&config.log_dir, date)?;
    let Some(last) = doc.entries.last_mut() else {
        bail!("no entries found for {date}");
    };

    last.project = entry.project;
    last.kind = entry.kind;
    last.summary = entry.summary;
    last.details = entry.details;

    save_day_document(config, &doc)
}

pub fn open_in_editor(config: &Config, date: NaiveDate) -> Result<()> {
    let doc = load_day_document(&config.log_dir, date)?;
    let path = save_day_document(config, &doc)?;
    let line = last_entry_line_number(&doc);
    launch_editor(config, &path, line)
}

pub fn collect_period_entries(root: &Path, start: NaiveDate, end: NaiveDate) -> Result<Vec<Entry>> {
    let mut entries = Vec::new();
    let mut current = start;
    while current <= end {
        let path = log_path(root, current);
        if path.exists() {
            let doc = load_day_document(root, current)?;
            entries.extend(doc.entries);
        }
        current += Duration::days(1);
    }
    Ok(entries)
}

pub fn collect_project_entries(root: &Path, project: &str) -> Result<Vec<Entry>> {
    let mut entries = load_all_entries(root)?;
    entries.retain(|entry| entry.project.as_deref() == Some(project));
    entries.sort_by(entry_sort_key);
    Ok(entries)
}

pub fn search_entries(root: &Path, pattern: &str) -> Result<Vec<Entry>> {
    let regex = Regex::new(pattern).with_context(|| format!("invalid regex `{pattern}`"))?;
    let mut matches = Vec::new();

    for entry in load_all_entries(root)? {
        if regex.is_match(&entry.summary) || entry.details.iter().any(|line| regex.is_match(line)) {
            matches.push(entry);
        }
    }

    matches.sort_by(entry_sort_key);
    Ok(matches)
}

pub fn lint_repository(root: &Path, config: &Config) -> Result<Vec<LintProblem>> {
    let mut problems = Vec::new();

    for item in WalkDir::new(root).follow_links(false) {
        let item = item?;
        let path = item.path();

        if item.file_type().is_symlink()
            && path.file_name().and_then(|name| name.to_str()) == Some("latest.md")
        {
            if let Err(error) = lint_latest_symlink(root, path) {
                problems.push(LintProblem {
                    path: path.to_path_buf(),
                    message: error.to_string(),
                });
            }
            continue;
        }

        if !item.file_type().is_file()
            || path.extension().and_then(|ext| ext.to_str()) != Some("md")
        {
            continue;
        }

        if let Err(error) = lint_file(root, path, config) {
            problems.push(LintProblem {
                path: path.to_path_buf(),
                message: error.to_string(),
            });
        }
    }

    problems.sort_by(|left, right| {
        left.path
            .cmp(&right.path)
            .then(left.message.cmp(&right.message))
    });
    Ok(problems)
}

pub fn format_entries(entries: &[Entry], sort: Option<SortMode>, include_all: bool) -> String {
    if entries.is_empty() {
        return "No entries found.".to_owned();
    }

    match sort {
        Some(SortMode::Project) => format_entries_by_project(entries, include_all),
        None => format_entries_by_day(entries, include_all),
    }
}

pub fn format_search_results(entries: &[Entry], include_all: bool) -> String {
    if entries.is_empty() {
        return "No entries found.".to_owned();
    }

    let mut lines = Vec::new();
    for entry in entries {
        lines.push(render_entry_line_with_date(
            entry.date,
            entry.timestamp,
            entry.project.as_deref(),
            &entry.kind,
            &entry.summary,
        ));
        if include_all {
            for detail in &entry.details {
                lines.push(format!("  {}", detail));
            }
        }
    }

    lines.join("\n")
}

pub fn print_emoji_section(section: EmojiSection) -> String {
    let group = emoji_group(section);
    let title = emoji_section_name(section);
    let mut out = String::new();
    out.push_str(&format!("# {title}\n\n"));
    out.push_str("| Emoji | Shortcode | Name |\n");
    out.push_str("| --- | --- | --- |\n");

    for emoji in group.emojis() {
        let shortcodes = emoji
            .shortcodes()
            .map(|shortcode| format!("`:{shortcode}:`"))
            .collect::<Vec<_>>();
        let shortcode_cell = if shortcodes.is_empty() {
            "-".to_owned()
        } else {
            shortcodes.join(", ")
        };
        out.push_str(&format!(
            "| {} | {} | {} |\n",
            emoji.as_str(),
            shortcode_cell,
            emoji.name()
        ));
    }

    out
}

pub fn work_week_range(anchor: NaiveDate) -> (NaiveDate, NaiveDate) {
    let start = anchor - Duration::days(anchor.weekday().num_days_from_monday() as i64);
    let end = start + Duration::days(4);
    (start, end)
}

pub fn month_range(anchor: NaiveDate) -> Result<(NaiveDate, NaiveDate)> {
    let start = NaiveDate::from_ymd_opt(anchor.year(), anchor.month(), 1)
        .ok_or_else(|| anyhow!("invalid month for {anchor}"))?;
    let (next_year, next_month) = if anchor.month() == 12 {
        (anchor.year() + 1, 1)
    } else {
        (anchor.year(), anchor.month() + 1)
    };
    let next_month_start = NaiveDate::from_ymd_opt(next_year, next_month, 1)
        .ok_or_else(|| anyhow!("invalid month"))?;
    Ok((start, next_month_start - Duration::days(1)))
}

pub fn year_range(anchor: NaiveDate) -> Result<(NaiveDate, NaiveDate)> {
    let start =
        NaiveDate::from_ymd_opt(anchor.year(), 1, 1).ok_or_else(|| anyhow!("invalid year"))?;
    let end =
        NaiveDate::from_ymd_opt(anchor.year(), 12, 31).ok_or_else(|| anyhow!("invalid year"))?;
    Ok((start, end))
}

fn collect_interactive_entry() -> Result<Option<String>> {
    use rustyline::DefaultEditor;
    use rustyline::error::ReadlineError;

    eprintln!("Enter your log entry. Press Ctrl-D on an empty prompt to save. Ctrl-C cancels.");

    let mut editor = DefaultEditor::new().context("failed to start interactive editor")?;
    let mut lines = Vec::new();

    loop {
        let prompt = if lines.is_empty() { "wrk> " } else { "... " };
        match editor.readline(prompt) {
            Ok(line) => lines.push(line),
            Err(ReadlineError::Eof) => break,
            Err(ReadlineError::Interrupted) => bail!("interactive entry cancelled"),
            Err(error) => return Err(error).context("failed to read interactive input"),
        }
    }

    if lines.iter().all(|line| line.trim().is_empty()) {
        return Ok(None);
    }

    Ok(Some(lines.join("\n")))
}

fn load_all_entries(root: &Path) -> Result<Vec<Entry>> {
    let mut entries = Vec::new();

    for item in WalkDir::new(root).follow_links(false) {
        let item = item?;
        if !item.file_type().is_file() {
            continue;
        }
        let path = item.path();
        if path.extension().and_then(|ext| ext.to_str()) != Some("md") {
            continue;
        }
        let Some(date) = date_from_path(root, path).ok() else {
            continue;
        };
        let doc = load_day_document(root, date)?;
        entries.extend(doc.entries);
    }

    entries.sort_by(entry_sort_key);
    Ok(entries)
}

fn lint_file(root: &Path, path: &Path, config: &Config) -> Result<()> {
    let expected_date = date_from_path(root, path)?;
    let raw =
        fs::read_to_string(path).with_context(|| format!("failed to read {}", path.display()))?;
    let lines: Vec<&str> = raw.lines().collect();

    if lines.first().copied() != Some(format!("# {expected_date}").as_str()) {
        bail!("title must be `# {expected_date}`");
    }

    let work_log_index = lines
        .iter()
        .position(|line| *line == "## Work Log")
        .ok_or_else(|| anyhow!("missing `## Work Log` section"))?;
    let notes_index = lines
        .iter()
        .position(|line| *line == "## Notes")
        .ok_or_else(|| anyhow!("missing `## Notes` section"))?;

    if notes_index <= work_log_index {
        bail!("`## Notes` must appear after `## Work Log`");
    }

    let mut seen_entry = false;
    for line in &lines[(work_log_index + 1)..notes_index] {
        if line.is_empty() {
            continue;
        }
        if parse_entry_line(line).is_some() {
            let parsed = parse_entry_line(line).expect("checked above");
            if !config.types.iter().any(|kind| kind == parsed.kind) {
                bail!("unknown type `{}`", parsed.kind);
            }
            seen_entry = true;
            continue;
        }
        if line.starts_with("  ") {
            if !seen_entry {
                bail!("indented continuation line must follow a log entry");
            }
            continue;
        }
        bail!("invalid work-log line `{line}`");
    }

    Ok(())
}

fn lint_latest_symlink(root: &Path, path: &Path) -> Result<()> {
    let canonical_root = root
        .canonicalize()
        .with_context(|| format!("failed to resolve {}", root.display()))?;
    let target =
        fs::read_link(path).with_context(|| format!("failed to read {}", path.display()))?;
    let resolved = if target.is_absolute() {
        target
            .canonicalize()
            .with_context(|| format!("broken latest symlink at {}", path.display()))?
    } else {
        path.parent()
            .unwrap_or(root)
            .join(target)
            .canonicalize()
            .with_context(|| format!("broken latest symlink at {}", path.display()))?
    };
    if !resolved.starts_with(&canonical_root) {
        bail!("latest symlink must point inside {}", root.display());
    }
    Ok(())
}

fn date_from_path(root: &Path, path: &Path) -> Result<NaiveDate> {
    let relative = path
        .strip_prefix(root)
        .with_context(|| format!("{} is outside {}", path.display(), root.display()))?;

    let components: Vec<_> = relative.iter().collect();
    if components.len() != 3 {
        bail!("path must be YYYY/MM/YYYY-MM-DD.md under the log root");
    }

    let year_dir = components[0]
        .to_str()
        .ok_or_else(|| anyhow!("invalid UTF-8 in path"))?;
    let month_dir = components[1]
        .to_str()
        .ok_or_else(|| anyhow!("invalid UTF-8 in path"))?;
    let file_name = components[2]
        .to_str()
        .ok_or_else(|| anyhow!("invalid UTF-8 in path"))?;

    let Some(stem) = file_name.strip_suffix(".md") else {
        bail!("file must end in .md");
    };

    let date = NaiveDate::parse_from_str(stem, "%Y-%m-%d")
        .with_context(|| format!("invalid date filename `{file_name}`"))?;
    if year_dir != format!("{:04}", date.year()) {
        bail!("year directory `{year_dir}` does not match filename date");
    }
    if month_dir != format!("{:02}", date.month()) {
        bail!("month directory `{month_dir}` does not match filename date");
    }
    Ok(date)
}

fn load_day_document(root: &Path, date: NaiveDate) -> Result<DayDocument> {
    let path = log_path(root, date);
    if !path.exists() {
        return Ok(DayDocument {
            date,
            entries: Vec::new(),
            notes: String::new(),
        });
    }

    let raw =
        fs::read_to_string(&path).with_context(|| format!("failed to read {}", path.display()))?;
    parse_day_document(date, &raw).with_context(|| format!("failed to parse {}", path.display()))
}

fn parse_day_document(expected_date: NaiveDate, raw: &str) -> Result<DayDocument> {
    let normalized = raw.replace("\r\n", "\n");
    let lines: Vec<&str> = normalized.lines().collect();
    if lines.is_empty() {
        bail!("file is empty");
    }
    let expected_title = format!("# {expected_date}");
    if lines[0] != expected_title {
        bail!("first line must be `{expected_title}`");
    }

    let work_log_index = lines
        .iter()
        .position(|line| *line == "## Work Log")
        .ok_or_else(|| anyhow!("missing `## Work Log` section"))?;
    let notes_index = lines
        .iter()
        .position(|line| *line == "## Notes")
        .ok_or_else(|| anyhow!("missing `## Notes` section"))?;
    if notes_index <= work_log_index {
        bail!("`## Notes` must appear after `## Work Log`");
    }

    let mut entries = Vec::new();
    let mut current: Option<Entry> = None;

    for line in &lines[(work_log_index + 1)..notes_index] {
        if line.is_empty() {
            continue;
        }
        if let Some(parsed) = parse_entry_line(line) {
            if let Some(entry) = current.take() {
                entries.push(entry);
            }
            current = Some(Entry {
                date: expected_date,
                timestamp: parsed.timestamp,
                project: parsed.project.map(ToOwned::to_owned),
                kind: parsed.kind.to_owned(),
                summary: parsed.summary.to_owned(),
                details: Vec::new(),
            });
            continue;
        }

        if let Some(entry) = current.as_mut() {
            if let Some(detail) = line.strip_prefix("  ") {
                entry.details.push(detail.to_owned());
                continue;
            }
        }

        bail!("invalid work-log line `{line}`");
    }

    if let Some(entry) = current {
        entries.push(entry);
    }

    let notes = if notes_index + 1 >= lines.len() {
        String::new()
    } else {
        lines[(notes_index + 1)..].join("\n")
    };

    Ok(DayDocument {
        date: expected_date,
        entries,
        notes,
    })
}

fn save_day_document(config: &Config, doc: &DayDocument) -> Result<PathBuf> {
    let path = log_path(&config.log_dir, doc.date);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("failed to create {}", parent.display()))?;
    }
    fs::write(&path, serialize_day_document(doc))
        .with_context(|| format!("failed to write {}", path.display()))?;
    update_latest_symlink(&config.log_dir, &path)?;
    Ok(path)
}

fn serialize_day_document(doc: &DayDocument) -> String {
    let mut out = String::new();
    out.push_str(&format!("# {}\n\n## Work Log\n\n", doc.date));

    for entry in &doc.entries {
        out.push_str(&render_entry_line(
            entry.timestamp,
            entry.project.as_deref(),
            &entry.kind,
            &entry.summary,
        ));
        out.push('\n');
        for detail in &entry.details {
            out.push_str("  ");
            out.push_str(detail);
            out.push('\n');
        }
    }

    out.push_str("\n## Notes\n");
    if !doc.notes.trim_end().is_empty() {
        out.push('\n');
        out.push_str(doc.notes.trim_end_matches('\n'));
        out.push('\n');
    } else {
        out.push('\n');
    }
    out
}

fn update_latest_symlink(root: &Path, target: &Path) -> Result<()> {
    let latest = root.join("latest.md");
    if latest.exists() || latest.symlink_metadata().is_ok() {
        fs::remove_file(&latest)
            .with_context(|| format!("failed to remove {}", latest.display()))?;
    }

    #[cfg(unix)]
    {
        use std::os::unix::fs::symlink;

        let relative_target = target
            .strip_prefix(root)
            .map(Path::to_path_buf)
            .unwrap_or_else(|_| target.to_path_buf());
        symlink(&relative_target, &latest)
            .with_context(|| format!("failed to create {}", latest.display()))?;
    }

    #[cfg(not(unix))]
    {
        fs::copy(target, &latest)
            .with_context(|| format!("failed to update {}", latest.display()))?;
    }

    Ok(())
}

fn launch_editor(config: &Config, path: &Path, line: usize) -> Result<()> {
    let editor = config
        .editor
        .as_deref()
        .ok_or_else(|| anyhow!("set `editor` in ~/.wrkrc or define $EDITOR/$VISUAL"))?;
    let mut parts =
        shlex::split(editor).ok_or_else(|| anyhow!("failed to parse editor command"))?;
    if parts.is_empty() {
        bail!("editor command is empty");
    }

    let program = parts.remove(0);
    let executable = Path::new(&program)
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or(&program);
    let mut command = Command::new(&program);
    command.args(parts);

    match executable {
        "vim" | "nvim" | "vi" | "nano" | "hx" | "helix" => {
            command.arg(format!("+{line}")).arg(path);
        }
        "code" | "codium" | "subl" | "mate" => {
            command
                .arg("--goto")
                .arg(format!("{}:{line}", path.display()));
        }
        _ => {
            command.arg(path);
        }
    }

    let status = command
        .status()
        .with_context(|| format!("failed to launch editor `{editor}`"))?;
    if !status.success() {
        bail!("editor exited with status {status}");
    }
    Ok(())
}

fn last_entry_line_number(doc: &DayDocument) -> usize {
    let mut line = 5;
    let mut last = line;
    for entry in &doc.entries {
        last = line;
        line += 1 + entry.details.len();
    }
    last
}

fn render_entry_line(
    timestamp: NaiveTime,
    project: Option<&str>,
    kind: &str,
    summary: &str,
) -> String {
    let project = project.unwrap_or("");
    format!(
        "- {} {}:{} {}",
        timestamp.format("%H:%M"),
        project,
        kind,
        summary
    )
}

fn render_entry_line_with_date(
    date: NaiveDate,
    timestamp: NaiveTime,
    project: Option<&str>,
    kind: &str,
    summary: &str,
) -> String {
    let project = project.unwrap_or("");
    format!(
        "- {} {} {}:{} {}",
        date,
        timestamp.format("%H:%M"),
        project,
        kind,
        summary
    )
    .replace("  :", " :")
}

fn log_path(root: &Path, date: NaiveDate) -> PathBuf {
    root.join(format!("{:04}", date.year()))
        .join(format!("{:02}", date.month()))
        .join(format!("{date}.md"))
}

fn parse_entry_line(line: &str) -> Option<ParsedLine<'_>> {
    static ENTRY_RE: OnceLock<Regex> = OnceLock::new();
    let regex = ENTRY_RE.get_or_init(|| {
        Regex::new(
            r"^- (?P<time>\d{2}:\d{2}) (?P<project>[^:\s]*):(?P<kind>[^:\s]+) (?P<summary>.+)$",
        )
        .expect("valid entry regex")
    });

    let captures = regex.captures(line)?;
    let timestamp = NaiveTime::parse_from_str(captures.name("time")?.as_str(), "%H:%M").ok()?;
    let project = captures
        .name("project")
        .map(|value| value.as_str())
        .filter(|value| !value.is_empty());
    let kind = captures.name("kind")?.as_str();
    let summary = captures.name("summary")?.as_str();

    Some(ParsedLine {
        timestamp,
        project,
        kind,
        summary,
    })
}

fn expand_emoji_shortcodes(line: &str) -> String {
    static SHORTCODE_RE: OnceLock<Regex> = OnceLock::new();
    let regex = SHORTCODE_RE
        .get_or_init(|| Regex::new(r":([A-Za-z0-9_+\-]+):").expect("valid shortcode regex"));

    regex
        .replace_all(line, |captures: &Captures<'_>| {
            let shortcode = &captures[1];
            emojis::get_by_shortcode(shortcode)
                .map(|emoji| emoji.as_str().to_owned())
                .unwrap_or_else(|| captures[0].to_owned())
        })
        .into_owned()
}

fn format_entries_by_day(entries: &[Entry], include_all: bool) -> String {
    let mut grouped: BTreeMap<NaiveDate, Vec<&Entry>> = BTreeMap::new();
    for entry in entries {
        grouped.entry(entry.date).or_default().push(entry);
    }

    let mut blocks = Vec::new();
    for (date, day_entries) in grouped {
        let mut block = String::new();
        block.push_str(&format!("# {date}\n\n## Work Log\n\n"));
        for entry in day_entries {
            block.push_str(&render_entry_line(
                entry.timestamp,
                entry.project.as_deref(),
                &entry.kind,
                &entry.summary,
            ));
            block.push('\n');
            if include_all {
                for detail in &entry.details {
                    block.push_str("  ");
                    block.push_str(detail);
                    block.push('\n');
                }
            }
        }
        blocks.push(block.trim_end().to_owned());
    }

    blocks.join("\n\n")
}

fn format_entries_by_project(entries: &[Entry], include_all: bool) -> String {
    let mut sorted = entries.to_vec();
    sorted.sort_by(|left, right| {
        project_key(left)
            .cmp(&project_key(right))
            .then(entry_sort_key(left, right))
    });

    let mut grouped: BTreeMap<String, Vec<Entry>> = BTreeMap::new();
    for entry in sorted {
        grouped
            .entry(project_label(&entry))
            .or_default()
            .push(entry);
    }

    let mut blocks = Vec::new();
    for (project, group_entries) in grouped {
        let mut block = String::new();
        block.push_str(&format!("## {project}\n\n"));
        for entry in group_entries {
            block.push_str(&render_entry_line_with_date(
                entry.date,
                entry.timestamp,
                entry.project.as_deref(),
                &entry.kind,
                &entry.summary,
            ));
            block.push('\n');
            if include_all {
                for detail in &entry.details {
                    block.push_str("  ");
                    block.push_str(detail);
                    block.push('\n');
                }
            }
        }
        blocks.push(block.trim_end().to_owned());
    }

    blocks.join("\n\n")
}

fn entry_sort_key(left: &Entry, right: &Entry) -> Ordering {
    left.date
        .cmp(&right.date)
        .then(left.timestamp.cmp(&right.timestamp))
        .then(left.project.cmp(&right.project))
        .then(left.kind.cmp(&right.kind))
        .then(left.summary.cmp(&right.summary))
}

fn project_key(entry: &Entry) -> (&str, NaiveDate, NaiveTime) {
    (
        entry.project.as_deref().unwrap_or(""),
        entry.date,
        entry.timestamp,
    )
}

fn project_label(entry: &Entry) -> String {
    entry
        .project
        .clone()
        .unwrap_or_else(|| "(no-project)".to_owned())
}

fn emoji_group(section: EmojiSection) -> emojis::Group {
    match section {
        EmojiSection::SmileysAndEmotion => emojis::Group::SmileysAndEmotion,
        EmojiSection::PeopleAndBody => emojis::Group::PeopleAndBody,
        EmojiSection::AnimalsAndNature => emojis::Group::AnimalsAndNature,
        EmojiSection::FoodAndDrink => emojis::Group::FoodAndDrink,
        EmojiSection::TravelAndPlaces => emojis::Group::TravelAndPlaces,
        EmojiSection::Activities => emojis::Group::Activities,
        EmojiSection::Objects => emojis::Group::Objects,
        EmojiSection::Symbols => emojis::Group::Symbols,
        EmojiSection::Flags => emojis::Group::Flags,
    }
}

fn emoji_section_name(section: EmojiSection) -> &'static str {
    match section {
        EmojiSection::SmileysAndEmotion => "Smileys and Emotion",
        EmojiSection::PeopleAndBody => "People and Body",
        EmojiSection::AnimalsAndNature => "Animals and Nature",
        EmojiSection::FoodAndDrink => "Food and Drink",
        EmojiSection::TravelAndPlaces => "Travel and Places",
        EmojiSection::Activities => "Activities",
        EmojiSection::Objects => "Objects",
        EmojiSection::Symbols => "Symbols",
        EmojiSection::Flags => "Flags",
    }
}

struct ParsedLine<'a> {
    timestamp: NaiveTime,
    project: Option<&'a str>,
    kind: &'a str,
    summary: &'a str,
}

#[cfg(test)]
mod tests {
    use chrono::NaiveDate;
    use tempfile::TempDir;

    use super::*;

    fn test_config(root: &TempDir) -> Config {
        Config {
            log_dir: root.path().to_path_buf(),
            default_project: None,
            default_type: "note".to_owned(),
            types: vec!["note".to_owned(), "build".to_owned()],
            editor: Some("true".to_owned()),
        }
    }

    #[test]
    fn entry_line_round_trips() {
        let timestamp = NaiveTime::from_hms_opt(9, 15, 0).unwrap();
        let rendered = render_entry_line(timestamp, Some("api"), "note", "shipped :rocket:");
        assert_eq!(rendered, "- 09:15 api:note shipped :rocket:");
        let parsed = parse_entry_line(&rendered).unwrap();
        assert_eq!(parsed.project, Some("api"));
        assert_eq!(parsed.kind, "note");
    }

    #[test]
    fn expands_emoji_shortcodes_in_entries() {
        let root = TempDir::new().unwrap();
        let config = test_config(&root);
        let entry = build_new_entry(&config, None, None, "Lunch :taco:").unwrap();
        assert_eq!(entry.summary, "Lunch 🌮");
    }

    #[test]
    fn saves_in_year_month_layout() {
        let root = TempDir::new().unwrap();
        let config = test_config(&root);
        let date = NaiveDate::from_ymd_opt(2026, 3, 4).unwrap();
        let entry = build_new_entry(&config, Some("api"), Some("note"), "started").unwrap();
        let path = append_entry(&config, date, entry).unwrap();
        assert_eq!(
            path,
            root.path().join("2026").join("03").join("2026-03-04.md")
        );
        assert!(root.path().join("latest.md").exists());
    }

    #[test]
    fn parses_and_preserves_note_section() {
        let raw =
            "# 2026-03-04\n\n## Work Log\n\n- 09:15 :note started\n  extra\n\n## Notes\n\nhello\n";
        let doc = parse_day_document(NaiveDate::from_ymd_opt(2026, 3, 4).unwrap(), raw).unwrap();
        assert_eq!(doc.entries.len(), 1);
        assert_eq!(doc.entries[0].details, vec!["extra"]);
        assert_eq!(doc.notes, "\nhello");
    }
}
