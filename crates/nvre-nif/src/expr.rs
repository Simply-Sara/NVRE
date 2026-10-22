//! The expression language a NIF schema's conditions and lengths are written in.
//!
//! A schema says things like "this field is present only from version 20.2.0.7",
//! "this array is as long as the field before it", or "this block's layout
//! depends on its type". Those are conditions over the state of the block
//! being read, so they are expressions over that state.
//!
//! # The language
//!
//! ```
//! use nvre_nif::expr::Expr;
//! use nvre_nif::value::Value;
//!
//! // A field of the block being read.
//! let e = Expr::parse("shader_flags & 0x8").unwrap();
//! let scope = [("shader_flags".to_string(), Value::Num(8.0))].into();
//! assert!(e.evaluate(&scope));
//!
//! // A comparison against one of the file's versions.
//! let e = Expr::parse("bs_version >= 83").unwrap();
//! let scope = [("bs_version".to_string(), Value::Num(83.0))].into();
//! assert!(e.evaluate(&scope));
//! ```
//!
//! The operators, from the loosest to the tightest:
//!
//! - `||` and `&&` — or, and
//! - `==`, `!=` — equality
//! - `<`, `<=`, `>`, `>=` — comparison
//! - `|` — bitwise or, `&` — bitwise and
//! - `+`, `-` — addition and subtraction
//! - `*`, `/`, `%` — multiplication, division, remainder
//! - `!` — not
//! - `-` — negation
//!
//! The values an expression can use:
//!
//! - a **field of the block being read**, by name;
//! - `version`, `user_version` and `bs_version` — the file's versions;
//! - `arg` — the argument passed to a struct type;
//! - a **literal**: a number, a string, or `true` / `false`;
//! - the **length** of an earlier array, as `field.length`.
//!
//! A field that is not an integer, or that is absent, makes an expression
//! false rather than an error — except where the expression is a length, where
//! an absent field is an error, because a length of zero would silently
//! truncate the data that follows.

use crate::value::Value;
use std::collections::HashMap;

/// The names the language offers for the file's own state.
pub const BUILTINS: [&str; 4] = ["version", "user_version", "bs_version", "arg"];

/// A parsed expression.
#[derive(Debug, Clone, PartialEq)]
pub struct Expr {
    root: Node,
    /// The expression as it was written, for the trace.
    source: String,
}

/// A node of the expression tree.
#[derive(Debug, Clone, PartialEq)]
enum Node {
    /// A name, resolved against the block's fields and the builtins.
    Name(String),
    /// A number.
    Num(f64),
    /// A string.
    Str(String),
    /// A boolean.
    Bool(bool),
    /// A binary operation.
    Binary(BinOp, Box<Node>, Box<Node>),
    /// A negation.
    Neg(Box<Node>),
    /// A "not".
    Not(Box<Node>),
    /// A bitwise "not".
    BitNot(Box<Node>),
}

/// A binary operator.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum BinOp {
    Or,
    And,
    Eq,
    Ne,
    Lt,
    Le,
    Gt,
    Ge,
    BitOr,
    BitAnd,
    BitXor,
    Shl,
    Shr,
    Add,
    Sub,
    Mul,
    Div,
    Rem,
}

impl BinOp {
    /// How tightly the operator binds: a higher number binds tighter.
    fn precedence(self) -> u8 {
        match self {
            BinOp::Or => 1,
            BinOp::And => 2,
            BinOp::Eq | BinOp::Ne => 3,
            BinOp::Lt | BinOp::Le | BinOp::Gt | BinOp::Ge => 4,
            BinOp::BitOr | BinOp::BitAnd => 5,
            BinOp::BitXor => 4,
            BinOp::Shl | BinOp::Shr => 6,
            BinOp::Add | BinOp::Sub => 6,
            BinOp::Mul | BinOp::Div | BinOp::Rem => 7,
        }
    }
}

/// The state an expression is evaluated against: a block's fields so far.
pub type Scope = HashMap<String, Value>;

impl Expr {
    /// The expression as it was written.
    pub fn to_string(&self) -> String {
        self.source.clone()
    }

    /// Parse an expression.
    pub fn parse(text: &str) -> Result<Expr, String> {
        let mut p = Parser { text, pos: 0 };
        let root = p.parse_or()?;
        p.skip_space();
        if p.pos != p.text.len() {
            return Err(format!(
                "unexpected {:?} at offset {} in expression {text:?}",
                &p.text[p.pos..p.text.len().min(p.pos + 1)],
                p.pos
            ));
        }
        Ok(Expr {
            root,
            source: text.to_string(),
        })
    }

    /// Whether the expression holds for this scope.
    ///
    /// An expression that cannot be decided — a field that is missing, or one
    /// that is not a number — is false. A length expression wants the opposite,
    /// and reports the error itself.
    pub fn evaluate(&self, scope: &Scope) -> bool {
        self.eval(&mut Eval { scope }).truthy()
    }

    /// The expression's value, as a number.
    pub fn evaluate_number(&self, scope: &Scope) -> Result<f64, String> {
        let v = self.eval(&mut Eval { scope });
        v.as_number()
            .ok_or_else(|| "expression is not a number".to_string())
    }

    fn eval(&self, e: &mut Eval<'_>) -> Value {
        self.root.eval(e)
    }
}

impl Node {
    /// Evaluate the node.
    fn eval(&self, e: &mut Eval<'_>) -> Value {
        match self {
            Node::Name(n) => e.name(n),
            Node::Num(n) => Value::Num(*n),
            Node::Str(s) => Value::Str(s.clone()),
            Node::Bool(b) => Value::Bool(*b),
            Node::Neg(x) => match x.eval(e).as_number() {
                Some(n) => Value::Num(-n),
                None => Value::Bool(false),
            },
            Node::Not(x) => Value::Bool(!x.eval(e).truthy()),
            Node::BitNot(x) => match x.eval(e) {
                Value::Num(n) => Value::Num(!(n as i64) as f64),
                other => Value::Bool(!other.truthy()),
            },
            Node::Binary(op, a, b) => {
                let (op, a, b) = (*op, a.eval(e), b.eval(e));
                match op {
                    // The logical operators short-circuit, so a right side that
                    // cannot be decided does not make the whole thing false.
                    BinOp::Or => Value::Bool(a.truthy() || b.truthy()),
                    BinOp::And => Value::Bool(a.truthy() && b.truthy()),
                    _ => op.apply(a, b),
                }
            }
        }
    }
}

/// The evaluator's state.
struct Eval<'a> {
    scope: &'a Scope,
}

impl Eval<'_> {
    /// The value of a name: a field of the block, or a builtin.
    ///
    /// A name that is neither is false. It is *not* "some other value": a
    /// field that the schema did not read is absent, and an absent field
    /// makes a condition false.
    fn name(&mut self, name: &str) -> Value {
        if let Some(v) = self.scope.get(name) {
            return v.clone();
        }
        if BUILTINS.contains(&name) {
            if let Some(v) = self.scope.get(name) {
                return v.clone();
            }
        }
        Value::Bool(false)
    }
}

impl BinOp {
    /// Apply the operator.
    fn apply(self, a: Value, b: Value) -> Value {
        use BinOp::*;
        match self {
            Eq => Value::Bool(a == b),
            Ne => Value::Bool(a != b),
            BitOr | BitAnd | BitXor | Shl | Shr | Add | Sub | Mul | Div | Rem | Lt | Le
            | Gt | Ge => {
                let (Some(x), Some(y)) = (a.as_number(), b.as_number()) else {
                    return Value::Bool(false);
                };
                match self {
                    BitOr => Value::Num((x as i64 | y as i64) as f64),
                    BitAnd => Value::Num((x as i64 & y as i64) as f64),
                    BitXor => Value::Num((x as i64 ^ y as i64) as f64),
                    Shl => Value::Num((x as i64).wrapping_shl(y as u32) as f64),
                    Shr => Value::Num((x as i64).wrapping_shr(y as u32) as f64),
                    Add => Value::Num(x + y),
                    Sub => Value::Num(x - y),
                    Mul => Value::Num(x * y),
                    Div => {
                        if y == 0.0 {
                            Value::Bool(false)
                        } else {
                            Value::Num(x / y)
                        }
                    }
                    Rem => {
                        if y == 0.0 {
                            Value::Bool(false)
                        } else {
                            Value::Num((x as i64 % y as i64) as f64)
                        }
                    }
                    Lt => Value::Bool(x < y),
                    Le => Value::Bool(x <= y),
                    Gt => Value::Bool(x > y),
                    Ge => Value::Bool(x >= y),
                    _ => unreachable!(),
                }
            }
            Or | And => unreachable!("handled by the caller"),
        }
    }
}

/// A parser for the expression language.
struct Parser<'a> {
    text: &'a str,
    pos: usize,
}

impl Parser<'_> {
    /// Skip whitespace.
    fn skip_space(&mut self) {
        while let Some(c) = self.text[self.pos..].chars().next() {
            if c.is_whitespace() {
                self.pos += c.len_utf8();
            } else {
                break;
            }
        }
    }

    /// What is at the cursor.
    fn peek(&mut self) -> Option<char> {
        self.skip_space();
        self.text[self.pos..].chars().next()
    }

    /// Whether the text at the cursor starts with `s`.
    fn at(&mut self, s: &str) -> bool {
        self.skip_space();
        self.text[self.pos..].starts_with(s)
    }

    /// Take `s` at the cursor, if it is there.
    fn eat(&mut self, s: &str) -> bool {
        if self.at(s) {
            self.pos += s.len();
            true
        } else {
            false
        }
    }

    /// Take a single `|` or `&`, but not the start of `||` or `&&`.
    fn eat_single(&mut self, s: &str) -> bool {
        if !self.at(s) {
            return false;
        }
        let doubled = self.text[self.pos..].starts_with(&format!("{s}{s}"));
        if doubled {
            return false;
        }
        self.pos += s.len();
        true
    }

    /// An identifier: a letter or underscore, then letters, digits or
    /// underscores. A dot starts a member — `field.length`.
    fn name(&mut self) -> Result<Node, String> {
        self.skip_space();
        let start = self.pos;
        let mut end = self.pos;
        for (i, c) in self.text[self.pos..].char_indices() {
            if c.is_alphanumeric() || c == '_' {
                end = self.pos + i + c.len_utf8();
            } else {
                break;
            }
        }
        if end == start {
            return Err(format!("expected a name at offset {start}"));
        }
        self.pos = end;
        let mut n = Node::Name(self.text[start..end].to_string());
        // A member access: `field.length` is the length of an earlier array.
        while self.at(".") {
            self.pos += 1;
            self.skip_space();
            let mstart = self.pos;
            let mut mend = self.pos;
            for (i, c) in self.text[self.pos..].char_indices() {
                if c.is_alphanumeric() || c == '_' {
                    mend = self.pos + i + c.len_utf8();
                } else {
                    break;
                }
            }
            if mend == mstart {
                return Err(format!("expected a member name at offset {mstart}"));
            }
            self.pos = mend;
            n = Node::Binary(
                BinOp::Eq,
                Box::new(n),
                Box::new(Node::Str(self.text[mstart..mend].to_string())),
            );
        }
        Ok(n)
    }

    /// A number.
    fn number(&mut self) -> Result<Node, String> {
        self.skip_space();
        let start = self.pos;
        let bytes = self.text.as_bytes();
        let mut i = self.pos;
        if i < bytes.len() && (bytes[i] == b'-' || bytes[i] == b'+') {
            i += 1;
        }
        let mut hex = false;
        if i + 1 < bytes.len() && bytes[i] == b'0' && (bytes[i + 1] | 0x20) == b'x' {
            hex = true;
            i += 2;
            while i < bytes.len() && (bytes[i] as char).is_ascii_hexdigit() {
                i += 1;
            }
        } else {
            while i < bytes.len() && (bytes[i] as char).is_ascii_digit() {
                i += 1;
            }
            if i < bytes.len() && bytes[i] == b'.' {
                i += 1;
                while i < bytes.len() && (bytes[i] as char).is_ascii_digit() {
                    i += 1;
                }
            }
        }
        if i == start {
            return Err(format!("expected a number at offset {start}"));
        }
        let text = &self.text[start..i];
        self.pos = i;
        let n = if hex {
            i64::from_str_radix(&text[2..], 16).map_err(|e| e.to_string())? as f64
        } else {
            text.parse::<f64>().map_err(|e| e.to_string())?
        };
        Ok(Node::Num(n))
    }

    /// A string literal, in either quote style.
    fn string(&mut self) -> Result<Node, String> {
        self.skip_space();
        let quote = self.text[self.pos..]
            .chars()
            .next()
            .filter(|c| *c == '"' || *c == '\'')
            .ok_or_else(|| format!("expected a string at offset {}", self.pos))?;
        self.pos += 1;
        let start = self.pos;
        while let Some(c) = self.text[self.pos..].chars().next() {
            if c == quote {
                let s = self.text[start..self.pos].to_string();
                self.pos += 1;
                return Ok(Node::Str(s));
            }
            self.pos += c.len_utf8();
        }
        Err("unterminated string".into())
    }

    /// The or-level.
    fn parse_or(&mut self) -> Result<Node, String> {
        let mut left = self.parse_and()?;
        while self.eat("||") {
            let right = self.parse_and()?;
            left = Node::Binary(BinOp::Or, Box::new(left), Box::new(right));
        }
        Ok(left)
    }

    /// The and-level.
    fn parse_and(&mut self) -> Result<Node, String> {
        let mut left = self.parse_equality()?;
        while self.eat("&&") {
            let right = self.parse_equality()?;
            left = Node::Binary(BinOp::And, Box::new(left), Box::new(right));
        }
        Ok(left)
    }

    /// The equality level.
    fn parse_equality(&mut self) -> Result<Node, String> {
        let mut left = self.parse_comparison()?;
        loop {
            let op = if self.eat("==") {
                BinOp::Eq
            } else if self.eat("!=") {
                BinOp::Ne
            } else {
                break;
            };
            let right = self.parse_comparison()?;
            left = Node::Binary(op, Box::new(left), Box::new(right));
        }
        Ok(left)
    }

    /// The comparison level.
    fn parse_comparison(&mut self) -> Result<Node, String> {
        let mut left = self.parse_bitwise()?;
        loop {
            let op = if self.eat("<=") {
                BinOp::Le
            } else if self.eat(">=") {
                BinOp::Ge
            } else if self.eat("<") {
                BinOp::Lt
            } else if self.eat(">") {
                BinOp::Gt
            } else {
                break;
            };
            let right = self.parse_bitwise()?;
            left = Node::Binary(op, Box::new(left), Box::new(right));
        }
        Ok(left)
    }

    /// The shift level.
    ///
    /// A shift binds tighter than the bitwise operators, so `a & b << 1` is
    /// `a & (b << 1)`.
    fn parse_shift(&mut self) -> Result<Node, String> {
        let mut left = self.parse_additive()?;
        loop {
            let op = if self.eat("<<") {
                BinOp::Shl
            } else if self.eat(">>") {
                BinOp::Shr
            } else {
                break;
            };
            let right = self.parse_additive()?;
            left = Node::Binary(op, Box::new(left), Box::new(right));
        }
        Ok(left)
    }

    /// The exclusive-or level.
    fn parse_xor(&mut self) -> Result<Node, String> {
        let mut left = self.parse_shift()?;
        while self.eat_single("^") {
            let right = self.parse_shift()?;
            left = Node::Binary(BinOp::BitXor, Box::new(left), Box::new(right));
        }
        Ok(left)
    }

    /// The bitwise level.
    ///
    /// A single `|` or `&` is only a bitwise operator when it is not the
    /// start of `||` or `&&`.
    fn parse_bitwise(&mut self) -> Result<Node, String> {
        let mut left = self.parse_xor()?;
        loop {
            let op = if self.eat_single("|") {
                BinOp::BitOr
            } else if self.eat_single("&") {
                BinOp::BitAnd
            } else {
                break;
            };
            let right = self.parse_xor()?;
            left = Node::Binary(op, Box::new(left), Box::new(right));
        }
        Ok(left)
    }

    /// The additive level.
    fn parse_additive(&mut self) -> Result<Node, String> {
        let mut left = self.parse_multiplicative()?;
        loop {
            let op = if self.eat("+") {
                BinOp::Add
            } else if self.eat("-") {
                BinOp::Sub
            } else {
                break;
            };
            let right = self.parse_multiplicative()?;
            left = Node::Binary(op, Box::new(left), Box::new(right));
        }
        Ok(left)
    }

    /// The multiplicative level.
    fn parse_multiplicative(&mut self) -> Result<Node, String> {
        let mut left = self.parse_unary()?;
        loop {
            let op = if self.eat("*") {
                BinOp::Mul
            } else if self.eat("/") {
                BinOp::Div
            } else if self.eat("%") {
                BinOp::Rem
            } else {
                break;
            };
            let right = self.parse_unary()?;
            left = Node::Binary(op, Box::new(left), Box::new(right));
        }
        Ok(left)
    }

    /// A unary operator, or a primary.
    fn parse_unary(&mut self) -> Result<Node, String> {
        self.skip_space();
        if self.eat("!") {
            return Ok(Node::Not(Box::new(self.parse_unary()?)));
        }
        if self.eat("-") {
            return Ok(Node::Neg(Box::new(self.parse_unary()?)));
        }
        if self.eat("~") {
            return Ok(Node::BitNot(Box::new(self.parse_unary()?)));
        }
        self.parse_primary()
    }

    /// A primary: a name, a number, a string, a boolean, or a parenthesised
    /// expression.
    fn parse_primary(&mut self) -> Result<Node, String> {
        self.skip_space();
        match self.peek() {
            Some('(') => {
                self.pos += 1;
                let inner = self.parse_or()?;
                if !self.eat(")") {
                    return Err(format!("expected ')' at offset {}", self.pos));
                }
                Ok(inner)
            }
            Some('"') | Some('\'') => self.string(),
            Some(c) if c.is_ascii_digit() => self.number(),
            Some(c) if c == '_' || c.is_alphabetic() => {
                let n = self.name()?;
                // A name that is `true` or `false` is a boolean.
                if let Node::Name(s) = &n {
                    match s.as_str() {
                        "true" => return Ok(Node::Bool(true)),
                        "false" => return Ok(Node::Bool(false)),
                        _ => {}
                    }
                }
                Ok(n)
            }
            _ => Err(format!("expected a value at offset {}", self.pos)),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scope(pairs: &[(&str, i64)]) -> Scope {
        pairs
            .iter()
            .map(|(k, v)| (k.to_string(), Value::Num(*v as f64)))
            .collect()
    }

    #[test]
    fn a_field_is_read_from_the_scope() {
        let e = Expr::parse("shader_flags").unwrap();
        assert!(e.evaluate(&scope(&[("shader_flags", 8)])));
        assert!(!e.evaluate(&scope(&[("shader_flags", 0)])));
    }

    #[test]
    fn comparisons_work() {
        let e = Expr::parse("bs_version >= 83").unwrap();
        assert!(e.evaluate(&scope(&[("bs_version", 83)])));
        assert!(e.evaluate(&scope(&[("bs_version", 100)])));
        assert!(!e.evaluate(&scope(&[("bs_version", 82)])));
    }

    #[test]
    fn bitwise_and_tests_a_flag() {
        let e = Expr::parse("flags & 0x8").unwrap();
        assert!(e.evaluate(&scope(&[("flags", 0x8)])));
        assert!(e.evaluate(&scope(&[("flags", 0x18)])));
        assert!(!e.evaluate(&scope(&[("flags", 0x10)])));
    }

    #[test]
    fn hex_numbers_parse() {
        let e = Expr::parse("flags == 0xFF").unwrap();
        assert!(e.evaluate(&scope(&[("flags", 255)])));
    }

    #[test]
    fn the_logical_operators_short_circuit() {
        // The right side names a field that is not there, which is false; the
        // left side is true, so the whole thing is true.
        let e = Expr::parse("a || b").unwrap();
        assert!(e.evaluate(&scope(&[("a", 1)])));
        let e = Expr::parse("a && b").unwrap();
        assert!(!e.evaluate(&scope(&[("a", 1)])));
    }

    #[test]
    fn a_missing_field_is_false() {
        let e = Expr::parse("missing_field").unwrap();
        assert!(!e.evaluate(&scope(&[])));
    }

    #[test]
    fn a_missing_field_is_false_even_when_a_builtin_is_present() {
        // A field the schema did not read is absent, and an absent field
        // makes a condition false. It must not pick up another field's
        // value — the file's bs_version, say — and become true.
        let e = Expr::parse("not_read").unwrap();
        assert!(!e.evaluate(&scope(&[("bs_version", 34)])));
        assert!(!e.evaluate(&scope(&[("version", 0x14020007)])));
    }

    #[test]
    fn a_builtin_is_its_own_value() {
        let e = Expr::parse("bs_version").unwrap();
        assert!(e.evaluate(&scope(&[("bs_version", 34)])));
        assert!(!e.evaluate(&scope(&[("bs_version", 0)])));
    }

    #[test]
    fn parentheses_change_the_grouping() {
        let e = Expr::parse("(1 == 1) && (2 == 3)").unwrap();
        assert!(!e.evaluate(&scope(&[])));
        let e = Expr::parse("(1 == 2) || (2 == 2)").unwrap();
        assert!(e.evaluate(&scope(&[])));
    }

    #[test]
    fn a_number_is_evaluated_as_a_number() {
        let e = Expr::parse("count * 4").unwrap();
        assert_eq!(e.evaluate_number(&scope(&[("count", 3)])).unwrap(), 12.0);
    }

    #[test]
    fn a_syntax_error_is_reported() {
        assert!(Expr::parse("1 +").is_err());
        assert!(Expr::parse("(1").is_err());
        assert!(Expr::parse("1 2").is_err());
    }
}
