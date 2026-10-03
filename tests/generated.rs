//! Histories written for this crate by real shells (`tests/fixtures/generated/`,
//! synthetic commands): bash 5.2 with and without `HISTTIMEFORMAT`, zsh 5.9
//! with and without `EXTENDED_HISTORY`. They cover multi-line commands,
//! continuation lines and bytes zsh escapes ("metafies").

use std::fs;
use std::path::Path;

use history::{parse, Format};

fn read(name: &str) -> Vec<u8> {
    fs::read(
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures/generated")
            .join(name),
    )
    .unwrap()
}

#[test]
fn bash_untimed_then_timed() {
    let parsed = parse(&read("bash_history"), Some("bash_history"));
    assert_eq!(parsed.format, Format::Bash);
    assert!(parsed.problems.is_empty(), "{:?}", parsed.problems);
    let commands: Vec<_> = parsed.commands.iter().map(|c| c.command.as_str()).collect();
    assert_eq!(
        commands,
        [
            "ls /tmp",
            "echo café",
            "uname -a",
            "for f in a b; do\n  echo $f\ndone",
            "echo \"one\ntwo\"",
            "exit",
        ]
    );
    let times: Vec<_> = parsed
        .commands
        .iter()
        .map(|c| c.time.and_then(|t| t.to_iso8601()))
        .collect();
    assert_eq!(times[..2], [None, None]);
    assert!(times[2..]
        .iter()
        .all(|t| t.as_deref() == Some("2026-10-03T13:39:02.0000000Z")));
}

#[test]
fn zsh_extended() {
    let parsed = parse(&read("zsh_extended_history"), None);
    assert_eq!(parsed.format, Format::Zsh);
    assert!(parsed.problems.is_empty(), "{:?}", parsed.problems);
    let commands: Vec<_> = parsed.commands.iter().map(|c| c.command.as_str()).collect();
    assert_eq!(
        commands,
        [
            "ls /tmp",
            "echo café",
            // ℵ (E2 84 B5) and ƒ (C6 92) were metafied.
            "print -r -- é€ℵ ƒ",
            // Typed as `echo one \` and `  two`.
            "echo one \\\n  two",
            "for f in a b; do\n  echo $f\ndone",
            "sleep 2",
            "uname -a",
        ]
    );
    let sleep = &parsed.commands[5];
    assert_eq!(sleep.duration_seconds, Some(2));
    assert_eq!(
        sleep.time.and_then(|t| t.to_iso8601()).as_deref(),
        Some("2026-10-03T13:38:47.0000000Z")
    );
    assert_eq!((sleep.line, parsed.commands[6].line), (9, 10));
}

#[test]
fn zsh_plain_named() {
    let data = read("zsh_history");
    let parsed = parse(&data, Some("/home/user/.zsh_history"));
    assert_eq!(parsed.format, Format::Zsh);
    let commands: Vec<_> = parsed.commands.iter().map(|c| c.command.as_str()).collect();
    assert_eq!(commands, ["ls /tmp", "print -r -- ℵ", "echo one \\\n  two"]);
    assert!(parsed.commands.iter().all(|c| c.time.is_none()));
    // Unnamed, it reads as bash: the escapes stay.
    assert_eq!(parse(&data, None).format, Format::Bash);
}
