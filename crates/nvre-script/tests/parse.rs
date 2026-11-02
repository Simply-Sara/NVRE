//! The parser: tokens to statements.
//!
//! Each test names the one rule it pins. The rules are the game's own, and the
//! ones that matter most are the surprising ones: declarations are collected
//! across the whole script before any statement is read, and a reference's
//! method call is one statement whether or not the writer put a space after
//! the dot.

use nvre_script::lex::{lex, Kind, Token};
use nvre_script::parse::{parse, Decl, Script, Stmt};

/// Zero a token's position.
///
/// A parser's output holds the tokens as they appeared, so a test comparing
/// whole tokens is comparing columns the test's own source string happens to
/// have. What the parser decided is the text and the kind; where a token sat is
/// the lexer's business, tested there. Normalising both sides keeps the parser
/// tests about the parser.
fn flat(t: &mut Token) {
    t.line = 0;
    t.col = 0;
}

/// The words of a line, whitespace dropped and positions zeroed.
fn words(source: &str) -> Vec<Token> {
    lex(source)
        .into_iter()
        .filter(|t| t.kind != Kind::Space)
        .map(|mut t| {
            flat(&mut t);
            t
        })
        .collect()
}

#[test]
fn a_scriptname_statement_carries_the_scripts_own_name() {
    let s = parse("ScriptName MyScript");
    assert_eq!(s.stmts, vec![Stmt::ScriptName("MyScript".to_string())]);
}

#[test]
fn scn_is_the_short_form_of_scriptname() {
    let s = parse("scn MyScript");
    assert_eq!(s.stmts, vec![Stmt::ScriptName("MyScript".to_string())]);
}

#[test]
fn a_begin_block_names_its_event_and_keeps_its_arguments() {
    let s = parse("Begin OnActivate rDoor");
    assert_eq!(
        s.stmts,
        vec![Stmt::Begin {
            event: "OnActivate".to_string(),
            args: words("rDoor"),
        }]
    );
}

#[test]
fn end_closes_the_block() {
    let s = parse("Begin GameMode\nEnd");
    assert_eq!(
        s.stmts,
        vec![
            Stmt::Begin {
                event: "GameMode".to_string(),
                args: Vec::new(),
            },
            Stmt::End
        ]
    );
}

#[test]
fn a_declaration_emits_no_statement_but_declares_the_variable() {
    let s = parse("short iCount\nref rDoor");
    assert!(s.stmts.is_empty(), "a declaration is not a statement");
    assert_eq!(
        s.decls,
        vec![
            Decl {
                name: "iCount".to_string(),
                is_int: true,
                line: 0,
            },
            Decl {
                name: "rDoor".to_string(),
                is_int: false,
                line: 1,
            },
        ]
    );
}

#[test]
fn a_declaration_inside_a_block_is_known_everywhere() {
    // Six of the shipped units declare a variable inside a block, and the
    // game's compiler knows it everywhere. So the parser collects every
    // declaration in a first pass, before it reads any statement.
    let s = parse("Begin GameMode\n  short iInside\n  set iInside to 1\nEnd");
    assert_eq!(
        s.decls.len(),
        1,
        "the declaration inside the block is collected"
    );
    assert_eq!(s.decls[0].name, "iInside");
    // And the statement after it is still read, so the two passes do not
    // consume each other.
    assert_eq!(
        s.stmts.len(),
        3,
        "Begin, the set, and End — the declaration is not a statement"
    );
}

#[test]
fn a_set_statement_separates_the_target_at_the_to() {
    let s = parse("set iCount to 5");
    assert_eq!(
        s.stmts,
        vec![Stmt::Set {
            on: None,
            target: words("iCount").into_iter().next().unwrap(),
            expr: words("5"),
        }]
    );
}

#[test]
fn let_is_the_other_spelling_of_set() {
    let s = parse("let iCount to 5");
    assert!(matches!(s.stmts.as_slice(), [Stmt::Set { .. }]));
}

#[test]
fn a_command_call_with_no_reference_names_the_command() {
    let s = parse("SetAV Player 5 10");
    assert_eq!(
        s.stmts,
        vec![Stmt::Call {
            on: None,
            command: "SetAV".to_string(),
            args: words("Player 5 10"),
        }]
    );
}

#[test]
fn a_references_method_call_is_one_statement_when_the_dot_has_no_space() {
    // `rDoor.Activate` — one word to the lexer, two names to the parser.
    let s = parse("rDoor.Activate");
    assert_eq!(
        s.stmts,
        vec![Stmt::Call {
            on: Some("rDoor".to_string()),
            command: "Activate".to_string(),
            args: Vec::new(),
        }]
    );
}

#[test]
fn a_references_method_call_is_one_statement_when_the_dot_has_a_space() {
    // `rDoor. Activate` — the GECK's own style, and the same statement.
    let s = parse("rDoor. Activate");
    assert_eq!(
        s.stmts,
        vec![Stmt::Call {
            on: Some("rDoor".to_string()),
            command: "Activate".to_string(),
            args: Vec::new(),
        }]
    );
}

#[test]
fn a_references_method_call_keeps_its_arguments() {
    let s = parse("Player.AddItem Bottlecap 5");
    assert_eq!(
        s.stmts,
        vec![Stmt::Call {
            on: Some("Player".to_string()),
            command: "AddItem".to_string(),
            args: words("Bottlecap 5"),
        }]
    );
}

#[test]
fn control_flow_reads_its_expression() {
    let s = parse("if iCount == 5");
    assert_eq!(s.stmts, vec![Stmt::If(words("iCount == 5"))]);
}

#[test]
fn else_and_endif_and_return_carry_no_expression() {
    let s = parse("else");
    assert_eq!(s.stmts, vec![Stmt::Else]);
    let s = parse("endif");
    assert_eq!(s.stmts, vec![Stmt::EndIf]);
    let s = parse("return");
    assert_eq!(s.stmts, vec![Stmt::Return]);
}

#[test]
fn a_comment_only_line_is_not_a_statement() {
    let s = parse("; just a note");
    assert!(s.stmts.is_empty());
}

#[test]
fn a_blank_line_is_not_a_statement() {
    let s = parse("\n\n");
    assert!(s.stmts.is_empty());
}

#[test]
fn a_line_the_parser_cannot_read_is_carried_with_its_line_number() {
    // `to` with no `set` in front of it: the parser has no statement for it,
    // so it is carried as unknown rather than dropped — a shorter body than
    // the source asked for would be a silent bug.
    let s = parse("set iCount to 1\nto nowhere");
    assert_eq!(s.stmts.len(), 2);
    match &s.stmts[1] {
        Stmt::Unknown { line, text } => {
            assert_eq!(*line, 1);
            assert_eq!(text, "to nowhere");
        }
        other => panic!("expected an unknown statement, got {other:?}"),
    }
}

#[test]
fn an_empty_script_has_no_declarations_and_no_statements() {
    let s: Script = parse("");
    assert!(s.decls.is_empty());
    assert!(s.stmts.is_empty());
}
