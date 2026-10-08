//! plaso's viminfo test files (Apache-2.0, `tests/fixtures/plaso/viminfo`
//! and `viminfo_alt`, plaso's `.viminfo` and `.viminfo_alt`): every entry
//! plaso's `text/viminfo` parser reads, read the same
//! (`tests/oracle/plaso-viminfo.tsv`, written from plaso's output: file,
//! section, item number, value, file, time).

use std::fs;
use std::path::Path;

use history::{parse, Format};

#[test]
fn every_entry_as_plaso_reads_it() {
    let mut got = Vec::new();
    for name in ["viminfo", "viminfo_alt"] {
        let data = fs::read(
            Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("tests/fixtures/plaso")
                .join(name),
        )
        .unwrap();
        let history = parse(&data, Some(&format!("home/bob/.{name}")));
        assert_eq!(history.format, Format::Viminfo);
        assert_eq!(history.problems, Vec::<String>::new());
        let mut numbers = std::collections::HashMap::new();
        for command in &history.commands {
            let section = command.section.unwrap();
            let number = numbers.entry(section).or_insert(0usize);
            got.push(format!(
                "{name}\t{section}\t{number}\t{}\t{}\t{}",
                command.command.replace('\n', "\\n"),
                command
                    .paths
                    .first()
                    .map(String::as_str)
                    .unwrap_or_default(),
                command
                    .time
                    .and_then(|t| t.ticks())
                    .map(|t| t / 10)
                    .unwrap_or_default()
            ));
            *number += 1;
        }
    }
    got.sort();
    let expected: Vec<&str> = include_str!("oracle/plaso-viminfo.tsv").lines().collect();
    assert_eq!(got, expected);
}
