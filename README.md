# history

Shell history files: bash's `.bash_history`, zsh's `.zsh_history` (or `.histfile`), fish's `fish_history` and PowerShell's PSReadLine history (`ConsoleHost_history.txt`), every command with its time, duration and paths when the shell wrote them. One dependency, its sibling `sootmark-common` (times).

```toml
[dependencies]
sootmark-history = "0.2"
```

```rust
let path = "/home/user/.zsh_history";
let parsed = history::parse(&std::fs::read(path)?, Some(path));
for command in &parsed.commands {
    println!("{:?} {:?}s {}", command.time, command.duration_seconds, command.command);
}
```

## What you get

- `parse(bytes, name)`: the shell, told from the content (fish's `- cmd:`, zsh's `: <seconds>:<duration>;`, bash's `#<seconds>`) or, for plain lines, from the file name; bash otherwise. `detect` gives the shell alone, `parse_as` reads as a shell you name.
- Every command with its line number, byte offset and text (lines joined with `\n`; bytes that aren't UTF-8 replaced):
  - **bash**: a command per line, untimed; with `HISTTIMEFORMAT`, `#<seconds>` before each command and every line up to the next time part of it (multi-line commands). A file that starts untimed and turns timed later is read both ways. A time with no command after it is reported.
  - **PowerShell** (PSReadLine, `…\PSReadLine\<host>Host_history.txt`, told by name): a command per line, untimed; a line ending in a backtick goes on to the next, as PSReadLine writes multi-line commands (the backtick removed, the lines kept); a byte order mark skipped.
  - **zsh**: `EXTENDED_HISTORY`'s start time and duration, or the bare command; a line ending in a backslash goes on to the next, as zsh reads it back. Bytes zsh escapes in its files ("metafied", `0x83` then the byte XORed with `0x20`) are restored, so non-ASCII commands read right.
  - **fish**: the command (`\\` and `\n` unescaped), its time and the `paths` fish recorded.
- Times in UTC, from the epoch seconds the shell wrote. A line that can't be read is reported in `problems`, never fatal.

Not yet: fish's format from before 2.0, and other shells (ksh, tcsh).

## How it's checked

- plaso's shell history test files (Apache-2.0, `tests/fixtures/plaso/`), as plaso's own tests expect them: command and problem counts, the commands and times plaso checks, bash files that turn timed partway, multi-line bash and zsh commands, fish's paths. plaso joins a multi-line bash command's lines with spaces; here they keep their line ends.
- PSReadLine histories written in its format (unit tests): blank lines, continued commands, `\r\n` endings, a byte order mark.
- Histories made for this crate by real shells (`tests/fixtures/generated/`, synthetic commands): bash 5.2 untimed then timed, with multi-line commands; zsh 5.9 extended and plain, with continuation lines, a duration and metafied bytes.
- Unit tests for blank lines, stray times, zsh's escapes and fish's escapes.
- Property tests: arbitrary bytes read as every shell's, and real files damaged anywhere, give commands or problems, never a panic.

## Licence

MIT or Apache-2.0, at your option. The test files in `tests/fixtures/plaso/` are plaso's, under the Apache licence 2.0; those in `tests/fixtures/generated/` were made for this crate and are under the crate's licence.
