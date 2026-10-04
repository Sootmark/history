//! plaso's shell history test files (Apache-2.0, `tests/fixtures/plaso/`),
//! read as plaso's own tests expect: how many commands, none unreadable,
//! and the commands and times they check. plaso joins a multi-line bash
//! command's lines with spaces; here they keep their line ends.

use std::fs;
use std::path::Path;

use history::{parse, Command, Format};

fn read(name: &str) -> Vec<u8> {
    fs::read(
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures/plaso")
            .join(name),
    )
    .unwrap()
}

fn time(command: &Command) -> String {
    command
        .time
        .and_then(|t| t.to_iso8601())
        .unwrap_or_default()
}

const MULTI_LINE: &str = "binary argument1 \"--params=\\\nparam1=foo,\nparam2=bar\n\" argument2";

#[test]
fn counts_as_plaso() {
    for (name, format, commands) in [
        ("bash_history", Format::Bash, 4),
        ("bash_history_desync", Format::Bash, 5),
        ("zsh_extended_history.txt", Format::Zsh, 4),
        ("fish_history", Format::Fish, 10),
    ] {
        let parsed = parse(&read(name), Some(name));
        assert_eq!(parsed.format, format, "{name}");
        assert_eq!(parsed.commands.len(), commands, "{name}");
        assert!(parsed.problems.is_empty(), "{name}: {:?}", parsed.problems);
    }
}

#[test]
fn bash_as_plaso() {
    let parsed = parse(&read("bash_history"), None);
    let first = &parsed.commands[0];
    assert_eq!(first.command, "/usr/lib/plaso");
    assert_eq!(time(first), "2013-10-01T12:36:17.0000000Z");
    assert_eq!((first.line, first.offset), (2, 12));
    let multi = &parsed.commands[3];
    assert_eq!(multi.command, MULTI_LINE);
    assert_eq!(time(multi), "2021-06-10T22:30:36.0000000Z");
}

#[test]
fn bash_desynchronised_as_plaso() {
    let parsed = parse(&read("bash_history_desync"), None);
    // Before the first time: a command, untimed.
    assert_eq!(parsed.commands[0].command, "/sbin/reboot");
    assert_eq!(parsed.commands[0].time, None);
    assert_eq!(parsed.commands[1].command, "/usr/lib/plaso");
    assert_eq!(time(&parsed.commands[1]), "2013-10-01T12:36:17.0000000Z");
    assert_eq!(parsed.commands[4].command, MULTI_LINE);
    assert_eq!(time(&parsed.commands[4]), "2021-06-10T22:30:36.0000000Z");
}

#[test]
fn zsh_as_plaso() {
    let parsed = parse(&read("zsh_extended_history.txt"), None);
    let first = &parsed.commands[0];
    assert_eq!(first.command, "cd plaso");
    assert_eq!(first.duration_seconds, Some(0));
    assert_eq!(time(first), "2016-03-12T08:26:50.0000000Z");
    // A line ending in a backslash goes on.
    assert_eq!(
        parsed.commands[2].command,
        "echo dfgdfg \\\n& touch /tmp/afile"
    );
    // The last line has no line end.
    assert_eq!(parsed.commands[3].command, "PYTHONPATH=. ./runtests.py");
}

#[test]
fn fish_as_plaso() {
    let parsed = parse(&read("fish_history"), None);
    let first = &parsed.commands[0];
    assert_eq!(first.command, "ll");
    assert_eq!(time(first), "2021-04-29T22:53:00.0000000Z");
    assert!(first.paths.is_empty());
    let copy = &parsed.commands[3];
    assert_eq!(copy.command, "cp *.txt test");
    assert_eq!(copy.paths, ["test"]);
    let last = parsed.commands.last().unwrap();
    assert_eq!(last.command, "cd plaso");
    assert_eq!(last.paths, ["plaso"]);
}

mod damage {
    use proptest::prelude::*;

    const FILES: [&str; 4] = [
        "bash_history",
        "bash_history_desync",
        "zsh_extended_history.txt",
        "fish_history",
    ];

    proptest! {
        /// Any bytes, read as any shell's: commands or problems, never a panic.
        #[test]
        fn arbitrary_bytes(data in proptest::collection::vec(any::<u8>(), 0..2_000)) {
            for format in [history::Format::Bash, history::Format::Zsh, history::Format::Fish, history::Format::PowerShell] {
                let _ = history::parse_as(&data, format);
            }
        }

        /// Real files, damaged anywhere.
        #[test]
        fn damaged_files(
            file in 0usize..FILES.len(),
            flips in proptest::collection::vec((0usize..500, any::<u8>()), 1..20),
        ) {
            let mut data = super::read(FILES[file]);
            for (at, byte) in flips {
                let len = data.len();
                data[at % len] = byte;
            }
            let _ = history::parse(&data, Some(FILES[file]));
        }
    }
}
