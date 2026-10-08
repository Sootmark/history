//! Vim's `.viminfo` (`_viminfo` on Windows): what was typed in Vim and which
//! files it edited, the commands an intruder ran in `:` and the files they
//! touched.
//!
//! Sections start with a comment (`# Command Line History (newest to
//! oldest):`). History entries are a line with a type character (`:`
//! command, `?` search, `=` expression, `@` input, `>` debug) then the
//! text; registers are `"<name>\t<type>\t<width>` then their lines, each
//! after a tab; file marks are `'<mark>  <line>  <column>  <file>`, the
//! jump list's `-'  <line>  <column>  <file>`. From Vim 8, a `|` line after
//! an entry gives its time (Unix seconds) and its value again, quoted.

use common::time::Ts;

use crate::{lines, text, Command, History, Line};

/// The sections read, by their comment.
const SECTIONS: [(&str, &str); 8] = [
    ("# Command Line History", "Command Line History"),
    ("# Search String History", "Search String History"),
    ("# Expression History", "Expression History"),
    ("# Input Line History", "Input Line History"),
    ("# Debug Line History", "Debug Line History"),
    ("# Registers", "Register"),
    ("# File marks", "File mark"),
    ("# Jumplist", "Jumplist"),
];

/// Whether `data` starts like a viminfo file.
pub(crate) fn is_viminfo(data: &[u8]) -> bool {
    data.starts_with(b"# This viminfo file")
}

pub(crate) fn read(data: &[u8], history: &mut History) {
    let mut section: Option<&'static str> = None;
    for line in lines(data) {
        let bytes = line.bytes;
        if bytes.starts_with(b"#") {
            let comment = text(bytes);
            section = SECTIONS
                .iter()
                .find(|(prefix, _)| comment.starts_with(prefix))
                .map(|(_, name)| *name);
            continue;
        }
        let Some(name) = section else {
            continue;
        };
        if let Some(bar) = bytes.strip_prefix(b"|") {
            stamp(bar, history);
            continue;
        }
        match name {
            "Register" => register(line, name, history),
            "File mark" | "Jumplist" => mark(line, name, history),
            _ => entry(line, name, history),
        }
    }
}

/// A history entry: its type character, then its text.
fn entry(line: Line<'_>, section: &'static str, history: &mut History) {
    let Some((_, rest)) = line.bytes.split_first() else {
        return;
    };
    if rest.is_empty() && line.bytes.is_empty() {
        return;
    }
    history.commands.push(command(line, section, text(rest)));
}

/// A register's header line starts it; its tab-indented lines follow.
fn register(line: Line<'_>, section: &'static str, history: &mut History) {
    if let Some(content) = line.bytes.strip_prefix(b"\t") {
        if let Some(last) = history
            .commands
            .last_mut()
            .filter(|c| c.section == Some(section))
        {
            if !last.command.is_empty() {
                last.command.push('\n');
            }
            last.command.push_str(&text(content));
        }
    } else if line.bytes.starts_with(b"\"") {
        history.commands.push(command(line, section, String::new()));
    }
}

/// A file mark or jump: the file is the fourth whitespace-separated field
/// on.
fn mark(line: Line<'_>, section: &'static str, history: &mut History) {
    let line_text = text(line.bytes);
    let mut rest = line_text.as_str();
    for _ in 0..3 {
        rest = rest.trim_start();
        rest = rest.split_once(char::is_whitespace).map_or("", |(_, r)| r);
    }
    let file = rest.trim().to_owned();
    if file.is_empty() {
        return;
    }
    let mut entry = command(line, section, String::new());
    entry.paths.push(file);
    history.commands.push(entry);
}

/// A `|` line: `2,<type>,<seconds>,…` after a history entry, `3,…,<seconds>,…`
/// after a register (its seventh field), `4,<mark>,<line>,<column>,<seconds>,…`
/// after a mark: the time of the entry before.
fn stamp(bar: &[u8], history: &mut History) {
    let fields: Vec<&[u8]> = bar.split(|&b| b == b',').collect();
    let at = match fields.first().copied() {
        Some(b"2") => 2,
        Some(b"3") => 6,
        Some(b"4") => 4,
        _ => return,
    };
    let Some(seconds) = fields.get(at).and_then(|f| crate::number::<i64>(f)) else {
        return;
    };
    if let Some(last) = history.commands.last_mut().filter(|c| c.time.is_none()) {
        last.time = Some(Ts::from_unix_seconds(seconds));
    }
}

fn command(line: Line<'_>, section: &'static str, value: String) -> Command {
    let mut command = Command::new(line, value);
    command.section = Some(section);
    command
}
