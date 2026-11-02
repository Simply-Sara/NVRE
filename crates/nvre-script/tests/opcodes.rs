//! The opcode tables, checked against the game's own bytes.
//!
//! The statement table is checked against the opcodes `FalloutNV.esm`'s
//! scripts actually use — the set measured by `examples/obscript_scope.rs` —
//! so a statement the emitter does not know is caught here rather than in the
//! game.
//!
//! The command table is checked the other way round: every entry must round
//! trip through the name and opcode lookups, and no two entries may share an
//! opcode. A duplicate opcode is two commands compiling to the same call,
//! which the game would answer by calling one of them.

use nvre_script::opcodes::{command_at, command_named, commands_starting_with, Event, Statement};

/// The statement opcodes `FalloutNV.esm`'s 2,576 scripts use, measured by
/// `examples/obscript_scope.rs`.
const STATEMENT_OPCODES_IN_THE_CORPUS: &[u16] = &[
    0x10, 0x11, 0x12, 0x15, 0x16, 0x17, 0x18, 0x19, 0x1A, 0x1C, 0x1D, 0x1E,
];

#[test]
fn every_statement_opcode_the_corpus_uses_has_a_statement() {
    for op in STATEMENT_OPCODES_IN_THE_CORPUS {
        assert!(
            Statement::from_opcode(*op).is_some(),
            "opcode 0x{op:04X} appears in FalloutNV.esm but the table has no statement for it"
        );
    }
}

#[test]
fn a_statement_round_trips_through_its_opcode() {
    for op in STATEMENT_OPCODES_IN_THE_CORPUS {
        let stmt = Statement::from_opcode(*op).expect("checked above");
        assert_eq!(stmt.opcode(), *op, "{stmt:?} does not round-trip");
    }
}

#[test]
fn the_opcodes_the_spec_does_not_name_are_unknown_not_wrong() {
    // 0x1A appears exactly once in the corpus and the spec does not name it.
    let stmt = Statement::from_opcode(0x1A).expect("it is in the statement range");
    assert_eq!(stmt, Statement::Unknown(0x1A));
    assert_eq!(stmt.keyword(), "", "an unnamed statement emits no keyword");
    assert_eq!(stmt.opcode(), 0x1A, "but its own bytes survive");
}

#[test]
fn an_opcode_outside_the_statement_range_is_not_a_statement() {
    assert!(Statement::from_opcode(0x0F).is_none());
    assert!(Statement::from_opcode(0x20).is_none());
    assert!(
        !Statement::is_statement(0x1000),
        "a command is not a statement"
    );
    assert!(Statement::is_statement(0x10));
}

#[test]
fn every_command_entry_round_trips_through_its_name_and_opcode() {
    for cmd in nvre_script::opcodes::COMMANDS {
        let by_name = command_named(cmd.name).expect("its own name finds it");
        assert_eq!(by_name.opcode, cmd.opcode, "{}", cmd.name);
        let by_op = command_at(cmd.opcode).expect("its own opcode finds it");
        assert_eq!(by_op.name, cmd.name, "0x{:04X}", cmd.opcode);
    }
}

#[test]
fn no_two_commands_share_an_opcode() {
    let mut seen = std::collections::HashMap::new();
    for cmd in nvre_script::opcodes::COMMANDS {
        if let Some(first) = seen.insert(cmd.opcode, cmd.name) {
            panic!(
                "opcode 0x{:04X} is both {} and {}",
                cmd.opcode, first, cmd.name
            );
        }
    }
}

#[test]
fn no_two_commands_share_a_name() {
    let mut seen = std::collections::HashSet::new();
    for cmd in nvre_script::opcodes::COMMANDS {
        let lower = cmd.name.to_ascii_lowercase();
        assert!(
            seen.insert(lower.clone()),
            "{} is listed twice, so a lookup is ambiguous",
            cmd.name
        );
    }
}

#[test]
fn a_command_lookup_ignores_case() {
    // A script may spell a command `setav`, `SetAV` or `SETAV`; the compiler
    // must treat all three as the same call.
    let cmd = command_named("setav").expect("lowercase finds it");
    assert_eq!(cmd.opcode, 0x100F);
    assert_eq!(command_named("SETAV").unwrap().opcode, 0x100F);
    assert_eq!(command_named("SetAv").unwrap().opcode, 0x100F);
}

#[test]
fn a_prefix_completion_returns_every_command_that_starts_with_it() {
    let got = commands_starting_with("add");
    assert!(!got.is_empty(), "the corpus settles several Add* commands");
    for cmd in &got {
        assert!(
            cmd.name.to_ascii_lowercase().starts_with("add"),
            "{} does not start with the prefix",
            cmd.name
        );
    }
    // Sorted by name, so the completion menu is stable frame to frame.
    let names: Vec<&str> = got.iter().map(|c| c.name).collect();
    let mut sorted = names.clone();
    sorted.sort_unstable();
    assert_eq!(names, sorted);
}

#[test]
fn the_events_the_corpus_uses_all_have_names() {
    // The events a `Begin` can open, as the corpus's `Begin` blocks spell them.
    // Each must map to a name, so a decompiled script reads as source.
    let codes = [
        0x0000, 0x0001, 0x0002, 0x0003, 0x0004, 0x0005, 0x0006, 0x0007, 0x0008, 0x0009, 0x000A,
        0x000B, 0x000C, 0x000F, 0x0010, 0x0011, 0x0012, 0x0013, 0x0014, 0x0015, 0x0016, 0x0017,
        0x0018, 0x0019, 0x001A, 0x001B, 0x001C, 0x001D, 0x001E, 0x001F, 0x0020, 0x0021, 0x0022,
        0x0023, 0x0024,
    ];
    for code in codes {
        let ev = Event::from_code(code).unwrap_or_else(|| panic!("code 0x{code:04X} has no name"));
        assert_eq!(ev.code(), code, "{} does not round-trip", ev.name());
        assert_eq!(
            Event::from_name(ev.name()).map(|e| e.code()),
            Some(code),
            "{} does not round-trip through its name",
            ev.name()
        );
    }
}

#[test]
fn an_event_name_lookup_ignores_case() {
    assert_eq!(Event::from_name("gamemode").map(|e| e.code()), Some(0x0000));
    assert_eq!(Event::from_name("GameMode").map(|e| e.code()), Some(0x0000));
    assert_eq!(
        Event::from_name("ONACTIVATE").map(|e| e.code()),
        Some(0x0002)
    );
    assert!(Event::from_name("OnNoSuchEvent").is_none());
}
