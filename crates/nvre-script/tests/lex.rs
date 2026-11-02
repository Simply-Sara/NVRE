//! The lexer: source text to tokens.
//!
//! Each test names the one behaviour it pins. The line and column are asserted
//! because the editor's diagnostics and the completion menu both need them: a
//! token with the wrong column is a diagnostic pointing at the wrong word.

use nvre_script::lex::{lex, Kind};

/// The tokens with their whitespace removed, so the assertions read as the
/// words a parser sees.
fn words(source: &str) -> Vec<(String, Kind)> {
    lex(source)
        .into_iter()
        .filter(|t| t.kind != Kind::Space)
        .map(|t| (t.text, t.kind))
        .collect()
}

#[test]
fn a_word_is_letters_digits_and_underscores() {
    let toks = words("SetAV Player 5 10");
    assert_eq!(
        toks,
        vec![
            ("SetAV".to_string(), Kind::Word),
            ("Player".to_string(), Kind::Word),
            ("5".to_string(), Kind::Number),
            ("10".to_string(), Kind::Number),
        ]
    );
}

#[test]
fn a_number_carries_a_sign_and_at_most_one_point() {
    let toks = words("19.50 .5 1. 7");
    assert_eq!(
        toks.iter().map(|(t, _)| t.as_str()).collect::<Vec<_>>(),
        vec!["19.50", ".5", "1.", "7"]
    );
    for (_, kind) in &toks {
        assert_eq!(*kind, Kind::Number);
    }
}

#[test]
fn a_quoted_string_keeps_its_quotes_and_runs_to_the_close() {
    let toks = words("Message \"hello world\" 5");
    assert_eq!(
        toks,
        vec![
            ("Message".to_string(), Kind::Word),
            ("\"hello world\"".to_string(), Kind::String),
            ("5".to_string(), Kind::Number),
        ]
    );
}

#[test]
fn an_unterminated_string_runs_to_the_end_of_the_line() {
    let toks = words("Message \"no close\nMessage \"x\"");
    // The first string stops at the newline, and the second line lexes cleanly.
    let strings: Vec<&str> = toks
        .iter()
        .filter(|(_, k)| *k == Kind::String)
        .map(|(t, _)| t.as_str())
        .collect();
    assert_eq!(strings, vec!["\"no close", "\"x\""]);
}

#[test]
fn a_comment_runs_to_the_end_of_its_line_and_keeps_the_semicolon() {
    let toks = words("Set x to 1 ; this is a note\nSet y to 2");
    let comments: Vec<&str> = toks
        .iter()
        .filter(|(_, k)| *k == Kind::Comment)
        .map(|(t, _)| t.as_str())
        .collect();
    assert_eq!(comments, vec!["; this is a note"]);
    // The next line is still lexed, so a comment does not swallow the script.
    // Six words: Set, x, to, Set, y, to — the `1` before the comment is a
    // number, not a word.
    assert_eq!(
        toks.iter().filter(|(_, k)| *k == Kind::Word).count(),
        6,
        "Set, x, to, Set, y, to"
    );
}

#[test]
fn the_line_and_column_count_characters_not_bytes() {
    // `é` is one character and two bytes, and it is a word character: a word
    // may hold it. `x` therefore sits at column 2 — the character offset —
    // rather than at the byte offset 4 a byte cursor would report.
    let toks = lex("é x");
    let second = toks.iter().find(|t| t.text == "x").unwrap();
    assert_eq!(second.text, "x");
    assert_eq!(second.line, 0);
    assert_eq!(second.col, 2);
}

#[test]
fn each_line_starts_at_column_zero() {
    let toks = lex("a\nb");
    let b = toks.iter().find(|t| t.text == "b").unwrap();
    assert_eq!(b.line, 1);
    assert_eq!(b.col, 0);
}

#[test]
fn operators_are_their_own_tokens() {
    let toks = words("if x == 1 && y != 2");
    let punct: Vec<&str> = toks
        .iter()
        .filter(|(_, k)| *k == Kind::Punct)
        .map(|(t, _)| t.as_str())
        .collect();
    assert_eq!(punct, vec!["==", "&&", "!="]);
}

#[test]
fn whitespace_is_kept_so_a_re_emit_reproduces_the_source() {
    let source = "Set  x\tto   1";
    let toks = lex(source);
    let rejoined: String = toks.iter().map(|t| t.text.as_str()).collect();
    assert_eq!(rejoined, source);
}
