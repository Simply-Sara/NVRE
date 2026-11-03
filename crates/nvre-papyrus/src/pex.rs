//! The `.pex` container: header, string table, debug info, objects.
//!
//! # Layout, as measured
//!
//! Every `.pex` in `Skyrim - Misc.bsa` (14,026 files) and in `Skyrim - Misc.bsa`
//! of the original edition (10,016 files) parses with this layout and consumes
//! its last byte; `tools/pexref.py` is the reference parse that checked it.
//!
//! ```text
//! magic            u32     0xFA57C0DE in the file's own byte order
//! major, minor     u8 u8   3.1 or 3.2 in Skyrim
//! game_id          u16     1 Skyrim, 2 Fallout 4, 3 Fallout 76, 4 Starfield
//! compile_time     u64     unix seconds
//! source_file      str     "DLC2ThirskDoorScript.psc"
//! user_name        str     the compiler's user
//! machine_name     str     the compiler's host
//! strings          u16 count, then count × str
//! debug            u8 present, then (if present) the debug block
//! user_flags       u16 count, then count × {sid name, u8 bit}
//! objects          u16 count, then count × object
//! ```
//!
//! where `str` is `u16 byte length` followed by that many bytes of
//! Windows-1252, and `sid` is a `u16` index into the string table.
//!
//! An object is:
//!
//! ```text
//! name             sid
//! size             u32     bytes from *after* `name` to the object's end
//! parent           sid     the base class, "" for none
//! doc              sid
//! [const_flag      u8      Fallout 4 only]
//! user_flags       u32
//! auto_state       sid     "" for the default state
//! [structs          Fallout 4 only]
//! variables        u16 count, then count × variable
//! properties       u16 count, then count × property
//! [guards           Starfield only]
//! states           u16 count, then count × state
//! ```
//!
//! # The `size` field does not count the name index
//!
//! Measured on `DLC2ThirskDoorScript.pex`: the object runs from offset 699 to
//! the end of the file at 1113, and the field says 412 — which is 1113 − 701,
//! the byte after the two-byte name index. A writer that counts from the
//! object's first byte writes 414 and the game's own loader rejects nothing
//! (it trusts the field counts rather than the size), but every tool that
//! compares scripts reports the file as rewritten. So the offset is measured
//! once, here, and [`Object::computed_size`] is what a writer should emit.
//!
//! # Endianness
//!
//! Skyrim's `.pex` is big-endian and Fallout 4's is little-endian, and both
//! spell the magic in their own order. [`Endianness`] is read from the magic
//! and carried in the parsed file, never assumed.

use crate::Error;
use nvre_util::cp1252;

/// Papyrus's magic number, as a number. Its on-disk bytes say which byte order
/// the file uses.
pub const MAGIC: u32 = 0xFA57_C0DE;

/// Byte order of a `.pex`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Endianness {
    /// Skyrim, Skyrim Special Edition, and Enderal. Magic bytes `FA 57 C0 DE`.
    Big,
    /// Fallout 4, Fallout 76 and Starfield. Magic bytes `DE C0 57 FA`.
    Little,
}

/// Which game's dialect a script is written in.
///
/// The value is the on-disk `game_id`, and the dialects differ in more than
/// the byte order: Fallout 4's object carries a constant-flag byte, a struct
/// table and a const flag per variable that Skyrim's does not have, and
/// Starfield adds a guard table.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Flavour {
    /// `game_id` 1 — Skyrim and Skyrim Special Edition. Big-endian.
    Skyrim,
    /// `game_id` 2 — Fallout 4. Little-endian.
    Fallout4,
    /// `game_id` 3 — Fallout 76. Little-endian.
    Fallout76,
    /// `game_id` 4 — Starfield. Little-endian, plus a guard table.
    Starfield,
}

impl Flavour {
    /// The dialect a `game_id` names, or `None` for one this crate has not seen.
    pub fn from_game_id(id: u16) -> Option<Flavour> {
        match id {
            1 => Some(Flavour::Skyrim),
            2 => Some(Flavour::Fallout4),
            3 => Some(Flavour::Fallout76),
            4 => Some(Flavour::Starfield),
            _ => None,
        }
    }

    /// Whether the dialect's object record has the Fallout-4-only fields.
    pub fn has_fo4_fields(self) -> bool {
        !matches!(self, Flavour::Skyrim)
    }

    /// Whether the dialect's object record has Starfield's guard table.
    pub fn has_guards(self) -> bool {
        matches!(self, Flavour::Starfield)
    }
}

/// A string table index.
///
/// Values in the bytecode name their text with an index into the file's string
/// table rather than repeating it, so a value keeps the index and the table
/// keeps the text. [`PexFile::text`] resolves one, and [`PexFile::intern`]
/// adds one.
pub type StrIndex = u16;

/// A tagged operand value.
///
/// The tag order is the compiler's own and is **not** the order the community
/// pages list: measured on `return ::State` in `DLC2ThirskDoorScript.pex`,
/// which writes `01 00 12` and ends the function, the order is
/// `0 none, 1 identifier, 2 string, 3 int, 4 float, 5 bool`. Reading `1` as a
/// boolean consumes one byte too few and shifts every later field.
#[derive(Debug, Clone, PartialEq)]
pub enum Value {
    /// Tag 0 — nothing.
    None,
    /// Tag 1 — a variable, parameter or local name.
    Identifier(StrIndex),
    /// Tag 2 — a literal string.
    String(StrIndex),
    /// Tag 3 — a 32-bit signed integer.
    Int(i32),
    /// Tag 4 — a 32-bit float.
    Float(f32),
    /// Tag 5 — a boolean.
    Bool(bool),
}

impl Value {
    /// The tag byte this value writes.
    pub fn tag(&self) -> u8 {
        match self {
            Value::None => 0,
            Value::Identifier(_) => 1,
            Value::String(_) => 2,
            Value::Int(_) => 3,
            Value::Float(_) => 4,
            Value::Bool(_) => 5,
        }
    }
}

/// The file header, without the string table that follows it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Header {
    /// Format major version: 3 in every shipped script.
    pub major: u8,
    /// Format minor version: 1 for Skyrim's original release, 2 for Special
    /// Edition, 9 for Fallout 4.
    pub minor: u8,
    /// Which game's dialect: see [`Flavour`].
    pub game_id: u16,
    /// When the script was compiled, as a unix timestamp.
    pub compile_time: u64,
    /// The source file's name, as the compiler was given it.
    pub source_file: String,
    /// The user the compiler ran as.
    pub user_name: String,
    /// The machine the compiler ran on.
    pub machine_name: String,
}

/// One function's entry in the debug block: which body it describes, and the
/// source line each of its instructions came from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DebugFunction {
    /// The object's name, as a string index.
    pub object: StrIndex,
    /// The state's name — `""` for the default state.
    pub state: StrIndex,
    /// The function's name.
    pub function: StrIndex,
    /// The compiler's function kind. 0 for an ordinary body; the values this
    /// crate has not seen are preserved rather than reinterpreted.
    pub kind: u8,
    /// The source line of each instruction, in instruction order.
    pub lines: Vec<u16>,
}

/// One Fallout-4-only property group in the debug block.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DebugPropertyGroup {
    /// The object this group belongs to.
    pub object: StrIndex,
    /// The group's name.
    pub group: StrIndex,
    /// The group's doc comment.
    pub doc: StrIndex,
    /// The group's user-flag bits.
    pub user_flags: u32,
    /// The properties in the group.
    pub names: Vec<StrIndex>,
}

/// One Fallout-4-only struct order in the debug block.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DebugStructOrder {
    /// The object this order belongs to.
    pub object: StrIndex,
    /// The struct's name.
    pub order: StrIndex,
    /// The members, in declaration order.
    pub names: Vec<StrIndex>,
}

/// The debug block: the compiler's line numbers and symbol naming.
///
/// It is optional — a script compiled with debug info suppressed has the
/// presence byte and nothing after it — and the game does not read it. It is
/// carried because a decompiler needs the line numbers to produce a source a
/// person can work with.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct DebugInfo {
    /// Whether the block is present at all.
    pub present: bool,
    /// The source file's modification time when it was compiled.
    pub modification_time: u64,
    /// One entry per function body.
    pub functions: Vec<DebugFunction>,
    /// Fallout 4 only: the property groups.
    pub property_groups: Vec<DebugPropertyGroup>,
    /// Fallout 4 only: the struct orders.
    pub struct_orders: Vec<DebugStructOrder>,
}

/// A user flag: a named bit the script can test.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UserFlag {
    /// The flag's name.
    pub name: StrIndex,
    /// The bit it sets, 0–31.
    pub bit: u8,
}

/// A script variable: a field of the object, not of a function.
#[derive(Debug, Clone, PartialEq)]
pub struct Variable {
    /// The variable's name.
    pub name: StrIndex,
    /// Its declared type.
    pub type_name: StrIndex,
    /// Its user-flag bits.
    pub user_flags: u32,
    /// Its initial value.
    pub initial_value: Value,
    /// Fallout 4 only: the const flag.
    pub const_flag: bool,
}

/// One property of an object.
///
/// A property is readable, writable, both, or an `auto` property backed by a
/// variable. A non-auto property carries a getter and/or a setter body instead
/// of a variable.
#[derive(Debug, Clone, PartialEq)]
pub struct Property {
    /// The property's name.
    pub name: StrIndex,
    /// Its declared type.
    pub type_name: StrIndex,
    /// Its doc comment.
    pub doc: StrIndex,
    /// Its user-flag bits.
    pub user_flags: u32,
    /// The raw flag byte. Bit 0 is readable, bit 1 writable, bit 2 auto.
    pub flags: u8,
    /// The backing variable, when bit 2 is set.
    pub auto_var: Option<StrIndex>,
    /// The getter body, when readable and not auto.
    pub getter: Option<Function>,
    /// The setter body, when writable and not auto.
    pub setter: Option<Function>,
}

impl Property {
    /// Whether the property can be read.
    pub fn is_readable(&self) -> bool {
        self.flags & 1 != 0
    }
    /// Whether the property can be written.
    pub fn is_writable(&self) -> bool {
        self.flags & 2 != 0
    }
    /// Whether the property is backed by a variable rather than accessors.
    pub fn is_auto(&self) -> bool {
        self.flags & 4 != 0
    }
}

/// A named type pair: a parameter or a local variable.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TypedName {
    /// The name.
    pub name: StrIndex,
    /// Its declared type.
    pub type_name: StrIndex,
}

/// One instruction: an opcode and its operands.
#[derive(Debug, Clone, PartialEq)]
pub struct Instruction {
    /// The opcode byte.
    pub opcode: u8,
    /// The opcode's fixed operands, in order.
    pub args: Vec<Value>,
    /// The operands after the count, for an opcode that takes a variable
    /// number of them (`callmethod`, `callparent`, `callstatic` and
    /// Starfield's guard opcodes).
    pub varargs: Vec<Value>,
}

impl Instruction {
    /// An instruction with no operands.
    pub fn new(opcode: u8) -> Self {
        Instruction {
            opcode,
            args: Vec::new(),
            varargs: Vec::new(),
        }
    }
}

/// A function body.
#[derive(Debug, Clone, PartialEq)]
pub struct Function {
    /// The function's name, as stored on the state that owns it.
    pub name: StrIndex,
    /// Its declared return type.
    pub return_type: StrIndex,
    /// Its doc comment.
    pub doc: StrIndex,
    /// Its user-flag bits.
    pub user_flags: u32,
    /// The raw flag byte. Bit 0 is global (static), bit 1 native.
    pub flags: u8,
    /// Its parameters, in order.
    pub params: Vec<TypedName>,
    /// Its locals, in order. The compiler's temporaries live here too, named
    /// `::temp0`, `::temp1` and so on.
    pub locals: Vec<TypedName>,
    /// Its instructions.
    pub instructions: Vec<Instruction>,
}

impl Function {
    /// Whether the function is global (static), with no `self`.
    pub fn is_global(&self) -> bool {
        self.flags & 1 != 0
    }
    /// Whether the function's body is native, i.e. implemented by the engine.
    pub fn is_native(&self) -> bool {
        self.flags & 2 != 0
    }
}

/// One state of an object. The default state has the empty name.
#[derive(Debug, Clone, PartialEq)]
pub struct State {
    /// The state's name; `""` for the default state.
    pub name: StrIndex,
    /// The functions that override the object's own in this state.
    pub functions: Vec<Function>,
}

/// A Fallout-4-only struct member.
#[derive(Debug, Clone, PartialEq)]
pub struct StructMember {
    /// The member's name.
    pub name: StrIndex,
    /// Its declared type.
    pub type_name: StrIndex,
    /// Its user-flag bits.
    pub user_flags: u32,
    /// Its initial value.
    pub initial_value: Value,
    /// Its const flag.
    pub const_flag: bool,
    /// Its doc comment.
    pub doc: StrIndex,
}

/// A Fallout-4-only struct definition.
#[derive(Debug, Clone, PartialEq)]
pub struct StructInfo {
    /// The struct's name.
    pub name: StrIndex,
    /// Its members, in order.
    pub members: Vec<StructMember>,
}

/// One script: the object the `.pex` describes.
///
/// A `.pex` holds a single object in every script Bethesda ships, but the
/// format allows a list, so the file keeps a list.
#[derive(Debug, Clone, PartialEq)]
pub struct Object {
    /// The script's name.
    pub name: StrIndex,
    /// The `size` field as stored. It counts the bytes after the name index;
    /// see the module's note. [`Object::computed_size`] recomputes it.
    pub size: u32,
    /// The base class, or `""` for none.
    pub parent: StrIndex,
    /// The script's doc comment.
    pub doc: StrIndex,
    /// Fallout 4 only: the const flag.
    pub const_flag: bool,
    /// The user-flag bits.
    pub user_flags: u32,
    /// The state the object starts in, or `""` for the default one.
    pub auto_state: StrIndex,
    /// Fallout 4 only: the struct definitions.
    pub structs: Vec<StructInfo>,
    /// The script's variables.
    pub variables: Vec<Variable>,
    /// The script's properties.
    pub properties: Vec<Property>,
    /// Starfield only: the guard names.
    pub guards: Vec<StrIndex>,
    /// The script's states.
    pub states: Vec<State>,
}

impl Object {
    /// The object's size in bytes, counted the way the field counts it: from
    /// the byte after the name index to the object's end.
    ///
    /// A writer should emit this rather than the `size` it read, because a
    /// source->bytecode comparison against the game's own files compares these
    /// bytes, and a wrong count shows up as a difference in this field rather
    /// than in the code that follows it.
    ///
    /// This writes the object with the same writer that produces the file, and
    /// measures the result. Counting the fields by hand instead would be a
    /// second copy of the layout, free to drift from the first — and the
    /// drift would be silent, because a size field is never read back.
    pub fn computed_size(&self, flavour: Flavour) -> u32 {
        let mut w = Writer::new(Endianness::Big);
        // Measured: DLC2ThirskDoorScript's object spans offsets 699..1113, so
        // 414 bytes, and its field says 412. That is the span minus the
        // two-byte name index — the field counts itself (4) and everything
        // after it, but not the name that precedes it. Across all 14,026
        // scripts in `Skyrim - Misc.bsa` this is the only reading that fits.
        write_object_body(&mut w, flavour, self);
        w.into_bytes().len() as u32 + 4
    }
}

/// A parsed `.pex`: the header, the string table, and the objects.
#[derive(Debug, Clone, PartialEq)]
pub struct PexFile {
    /// The byte order the file uses, read from its magic.
    pub endianness: Endianness,
    /// The dialect, read from the header's `game_id`.
    pub flavour: Flavour,
    /// The header fields.
    pub header: Header,
    /// The string table, in file order. Indices into this are what every
    /// value and every name field holds.
    pub strings: Vec<String>,
    /// The debug block.
    pub debug: DebugInfo,
    /// The user flags.
    pub user_flags: Vec<UserFlag>,
    /// The script objects.
    pub objects: Vec<Object>,
}

impl PexFile {
    /// The text of a string index.
    pub fn text(&self, index: StrIndex) -> Result<&str, Error> {
        self.strings
            .get(index as usize)
            .map(|s| s.as_str())
            .ok_or(Error::BadStringIndex {
                index,
                len: self.strings.len(),
            })
    }

    /// The text of a string index, or `""` when it is out of range.
    ///
    /// For display: a reader that has already validated the file has no
    /// out-of-range index, and a reporter printing a malformed one would
    /// rather show nothing than fail.
    pub fn text_or_empty(&self, index: StrIndex) -> &str {
        self.strings
            .get(index as usize)
            .map(|s| s.as_str())
            .unwrap_or("")
    }

    /// The index of `text` in the string table, adding it if it is new.
    ///
    /// This is how a compiler builds its table: the first string to need an
    /// entry gets the lowest free index, so the table is in first-use order.
    pub fn intern(&mut self, text: &str) -> Result<StrIndex, Error> {
        if let Some(i) = self.strings.iter().position(|s| s == text) {
            return Ok(i as StrIndex);
        }
        if self.strings.len() >= u16::MAX as usize {
            return Err(Error::Unsupported(
                "the string table is full (65535 entries)".into(),
            ));
        }
        self.strings.push(text.to_string());
        Ok((self.strings.len() - 1) as StrIndex)
    }

    /// Every object whose `size` field disagrees with the bytes it holds.
    ///
    /// A correct file has none; a file written by a tool that counted from the
    /// wrong offset has one per object. Reported rather than repaired, so a
    /// comparison against the game's own files can say which side is wrong.
    pub fn size_mismatches(&self) -> Vec<(StrIndex, u32, u32)> {
        self.objects
            .iter()
            .filter_map(|o| {
                let c = o.computed_size(self.flavour);
                (c != o.size).then_some((o.name, o.size, c))
            })
            .collect()
    }

    /// Check everything [`PexFile::write`] relies on.
    ///
    /// Each rule here is one the reader enforces structurally — a readable
    /// property always reads a getter body, so a model without one is not a
    /// model of any file that exists.
    fn validate(&self) -> Result<(), Error> {
        for o in &self.objects {
            for p in &o.properties {
                let name = self.text_or_empty(p.name);
                if p.is_auto() {
                    if p.auto_var.is_none() {
                        return Err(Error::Layout(format!(
                            "property {name:?} is an auto property with no backing variable"
                        )));
                    }
                } else {
                    if p.is_readable() && p.getter.is_none() {
                        return Err(Error::Layout(format!(
                            "property {name:?} is readable but has no getter body"
                        )));
                    }
                    if p.is_writable() && p.setter.is_none() {
                        return Err(Error::Layout(format!(
                            "property {name:?} is writable but has no setter body"
                        )));
                    }
                }
            }
            for s in &o.states {
                for f in &s.functions {
                    self.validate_function(f)?;
                }
            }
            for p in &o.properties {
                if let Some(g) = &p.getter {
                    self.validate_function(g)?;
                }
                if let Some(s) = &p.setter {
                    self.validate_function(s)?;
                }
            }
        }
        Ok(())
    }

    fn validate_function(&self, f: &Function) -> Result<(), Error> {
        let name = self.text_or_empty(f.name);
        for i in &f.instructions {
            let info = opcode_info(i.opcode, self.flavour).ok_or(Error::BadOpcode(i.opcode))?;
            if !info.varargs && !i.varargs.is_empty() {
                return Err(Error::Layout(format!(
                    "{name:?}: {} takes no vararg list, but this instruction has {} entries",
                    info.name,
                    i.varargs.len()
                )));
            }
            if i.args.len() != info.args as usize {
                return Err(Error::Layout(format!(
                    "{name:?}: {} takes {} operand(s), this instruction has {}",
                    info.name,
                    info.args,
                    i.args.len()
                )));
            }
        }
        Ok(())
    }
}

/// The endian-aware reader over a `.pex`.
///
/// `nvre_util::Cursor` reads little-endian only, and one of the two dialects
/// is big-endian, so the container carries its own reader rather than making
/// every call site pass a byte order.
pub(crate) struct Reader<'a> {
    data: &'a [u8],
    /// How far the reader is.
    pub pos: usize,
    e: Endianness,
}

impl<'a> Reader<'a> {
    pub fn new(data: &'a [u8], e: Endianness) -> Self {
        Reader { data, pos: 0, e }
    }

    fn take(&mut self, n: usize) -> Result<&'a [u8], Error> {
        let end = self.pos.checked_add(n).filter(|e| *e <= self.data.len());
        let Some(end) = end else {
            return Err(Error::Truncated(format!(
                "wanted {n} bytes at {}, file is {}",
                self.pos,
                self.data.len()
            )));
        };
        let s = &self.data[self.pos..end];
        self.pos = end;
        Ok(s)
    }

    pub fn u8(&mut self) -> Result<u8, Error> {
        Ok(self.take(1)?[0])
    }

    pub fn u16(&mut self) -> Result<u16, Error> {
        let b = self.take(2)?;
        Ok(match self.e {
            Endianness::Big => u16::from_be_bytes([b[0], b[1]]),
            Endianness::Little => u16::from_le_bytes([b[0], b[1]]),
        })
    }

    pub fn u32(&mut self) -> Result<u32, Error> {
        let b = self.take(4)?;
        Ok(match self.e {
            Endianness::Big => u32::from_be_bytes([b[0], b[1], b[2], b[3]]),
            Endianness::Little => u32::from_le_bytes([b[0], b[1], b[2], b[3]]),
        })
    }

    pub fn i32(&mut self) -> Result<i32, Error> {
        Ok(self.u32()? as i32)
    }

    pub fn u64(&mut self) -> Result<u64, Error> {
        let b = self.take(8)?;
        let a: [u8; 8] = b.try_into().unwrap();
        Ok(match self.e {
            Endianness::Big => u64::from_be_bytes(a),
            Endianness::Little => u64::from_le_bytes(a),
        })
    }

    pub fn f32(&mut self) -> Result<f32, Error> {
        Ok(f32::from_bits(self.u32()?))
    }

    /// A length-prefixed Windows-1252 string.
    pub fn string(&mut self) -> Result<String, Error> {
        let n = self.u16()? as usize;
        Ok(cp1252::decode(self.take(n)?))
    }
}

/// A `.pex` writer: the same fields back out, in the order they came in.
///
/// The writer emits the model as it stands, including each object's `size`
/// field as stored — so a parse followed by a write reproduces the input byte
/// for byte, which is what the round-trip test asserts. A compiler building a
/// file from nothing sets `size` from [`Object::computed_size`].
pub struct Writer {
    out: Vec<u8>,
    e: Endianness,
}

impl Writer {
    /// A writer for a file in the given byte order.
    pub fn new(e: Endianness) -> Self {
        Writer { out: Vec::new(), e }
    }

    /// The bytes written so far.
    pub fn into_bytes(self) -> Vec<u8> {
        self.out
    }

    fn u8(&mut self, v: u8) {
        self.out.push(v);
    }
    fn u16(&mut self, v: u16) {
        match self.e {
            Endianness::Big => self.out.extend_from_slice(&v.to_be_bytes()),
            Endianness::Little => self.out.extend_from_slice(&v.to_le_bytes()),
        }
    }
    fn u32(&mut self, v: u32) {
        match self.e {
            Endianness::Big => self.out.extend_from_slice(&v.to_be_bytes()),
            Endianness::Little => self.out.extend_from_slice(&v.to_le_bytes()),
        }
    }
    fn u64(&mut self, v: u64) {
        match self.e {
            Endianness::Big => self.out.extend_from_slice(&v.to_be_bytes()),
            Endianness::Little => self.out.extend_from_slice(&v.to_le_bytes()),
        }
    }
    fn string(&mut self, s: &str) -> Result<(), Error> {
        let bytes = cp1252::encode(s).ok_or_else(|| {
            Error::Unsupported(format!(
                "{s:?} has a character Windows-1252 cannot represent"
            ))
        })?;
        if bytes.len() > u16::MAX as usize {
            return Err(Error::Unsupported(format!(
                "a {}-byte string exceeds the u16 length prefix",
                bytes.len()
            )));
        }
        self.u16(bytes.len() as u16);
        self.out.extend_from_slice(&bytes);
        Ok(())
    }
}

impl PexFile {
    /// Parse a `.pex`.
    pub fn parse(data: &[u8]) -> Result<PexFile, Error> {
        if data.len() < 4 {
            return Err(Error::NotPex("shorter than its magic".into()));
        }
        // Both dialects spell the magic in their own byte order, so the first
        // four bytes say which one this is.
        let be = u32::from_be_bytes([data[0], data[1], data[2], data[3]]);
        let endianness = if be == MAGIC {
            Endianness::Big
        } else if u32::from_le_bytes([data[0], data[1], data[2], data[3]]) == MAGIC {
            Endianness::Little
        } else {
            return Err(Error::NotPex(format!(
                "magic {:02x}{:02x}{:02x}{:02x} is not FA57C0DE in either byte order",
                data[0], data[1], data[2], data[3]
            )));
        };
        let mut r = Reader::new(data, endianness);
        r.u32()?; // magic, already read

        let major = r.u8()?;
        let minor = r.u8()?;
        let game_id = r.u16()?;
        let compile_time = r.u64()?;
        let source_file = r.string()?;
        let user_name = r.string()?;
        let machine_name = r.string()?;
        let header = Header {
            major,
            minor,
            game_id,
            compile_time,
            source_file,
            user_name,
            machine_name,
        };

        let flavour = Flavour::from_game_id(game_id).ok_or_else(|| {
            Error::Unsupported(format!("game id {game_id} is not one this crate knows"))
        })?;

        let n = r.u16()? as usize;
        let mut strings = Vec::with_capacity(n);
        for _ in 0..n {
            strings.push(r.string()?);
        }

        let debug = read_debug(&mut r, flavour)?;

        let n = r.u16()? as usize;
        let mut user_flags = Vec::with_capacity(n);
        for _ in 0..n {
            user_flags.push(UserFlag {
                name: r.u16()?,
                bit: r.u8()?,
            });
        }

        let n = r.u16()? as usize;
        let mut objects = Vec::with_capacity(n);
        for _ in 0..n {
            objects.push(read_object(&mut r, flavour)?);
        }

        if r.pos != data.len() {
            return Err(Error::Layout(format!(
                "{} byte(s) left after the last object (at {} of {})",
                data.len() - r.pos,
                r.pos,
                data.len()
            )));
        }
        Ok(PexFile {
            endianness,
            flavour,
            header,
            strings,
            debug,
            user_flags,
            objects,
        })
    }

    /// Write the file back out.
    ///
    /// Refuses a file that could not have been read: a readable property with
    /// no getter, a writable one with no setter, an auto property with no
    /// backing variable, or a vararg list on an opcode that takes none. Those
    /// are checked up front rather than discovered halfway through, because
    /// half a written `.pex` is a file the game loads and runs.
    pub fn write(&self) -> Result<Vec<u8>, Error> {
        self.validate()?;
        let mut w = Writer::new(self.endianness);
        w.u32(MAGIC);
        w.u8(self.header.major);
        w.u8(self.header.minor);
        w.u16(self.header.game_id);
        w.u64(self.header.compile_time);
        w.string(&self.header.source_file)?;
        w.string(&self.header.user_name)?;
        w.string(&self.header.machine_name)?;

        if self.strings.len() > u16::MAX as usize {
            return Err(Error::Unsupported("the string table is too large".into()));
        }
        w.u16(self.strings.len() as u16);
        for s in &self.strings {
            w.string(s)?;
        }

        write_debug(&mut w, self)?;
        w.u16(self.user_flags.len() as u16);
        for f in &self.user_flags {
            w.u16(f.name);
            w.u8(f.bit);
        }

        w.u16(self.objects.len() as u16);
        for o in &self.objects {
            write_object(&mut w, self, o)?;
        }
        Ok(w.into_bytes())
    }
}

fn read_debug(r: &mut Reader<'_>, flavour: Flavour) -> Result<DebugInfo, Error> {
    let mut d = DebugInfo {
        present: r.u8()? != 0,
        ..Default::default()
    };
    if !d.present {
        return Ok(d);
    }
    d.modification_time = r.u64()?;
    let n = r.u16()? as usize;
    for _ in 0..n {
        let object = r.u16()?;
        let state = r.u16()?;
        let function = r.u16()?;
        let kind = r.u8()?;
        let k = r.u16()? as usize;
        let mut lines = Vec::with_capacity(k);
        for _ in 0..k {
            lines.push(r.u16()?);
        }
        d.functions.push(DebugFunction {
            object,
            state,
            function,
            kind,
            lines,
        });
    }
    // Skyrim's debug block ends after the function lines. Fallout 4 appends
    // property groups and struct orders.
    if flavour == Flavour::Skyrim {
        return Ok(d);
    }
    let n = r.u16()? as usize;
    for _ in 0..n {
        let object = r.u16()?;
        let group = r.u16()?;
        let doc = r.u16()?;
        let user_flags = r.u32()?;
        let k = r.u16()? as usize;
        let mut names = Vec::with_capacity(k);
        for _ in 0..k {
            names.push(r.u16()?);
        }
        d.property_groups.push(DebugPropertyGroup {
            object,
            group,
            doc,
            user_flags,
            names,
        });
    }
    let n = r.u16()? as usize;
    for _ in 0..n {
        let object = r.u16()?;
        let order = r.u16()?;
        let k = r.u16()? as usize;
        let mut names = Vec::with_capacity(k);
        for _ in 0..k {
            names.push(r.u16()?);
        }
        d.struct_orders.push(DebugStructOrder {
            object,
            order,
            names,
        });
    }
    Ok(d)
}

fn write_debug(w: &mut Writer, f: &PexFile) -> Result<(), Error> {
    let d = &f.debug;
    w.u8(d.present as u8);
    if !d.present {
        return Ok(());
    }
    w.u64(d.modification_time);
    w.u16(d.functions.len() as u16);
    for e in &d.functions {
        w.u16(e.object);
        w.u16(e.state);
        w.u16(e.function);
        w.u8(e.kind);
        w.u16(e.lines.len() as u16);
        for l in &e.lines {
            w.u16(*l);
        }
    }
    if f.flavour == Flavour::Skyrim {
        return Ok(());
    }
    w.u16(d.property_groups.len() as u16);
    for g in &d.property_groups {
        w.u16(g.object);
        w.u16(g.group);
        w.u16(g.doc);
        w.u32(g.user_flags);
        w.u16(g.names.len() as u16);
        for n in &g.names {
            w.u16(*n);
        }
    }
    w.u16(d.struct_orders.len() as u16);
    for s in &d.struct_orders {
        w.u16(s.object);
        w.u16(s.order);
        w.u16(s.names.len() as u16);
        for n in &s.names {
            w.u16(*n);
        }
    }
    Ok(())
}

fn read_value(r: &mut Reader<'_>) -> Result<Value, Error> {
    // The tag order is the compiler's own enum; see `Value`'s doc comment.
    let tag = r.u8()?;
    Ok(match tag {
        0 => Value::None,
        1 => Value::Identifier(r.u16()?),
        2 => Value::String(r.u16()?),
        3 => Value::Int(r.i32()?),
        4 => Value::Float(r.f32()?),
        5 => Value::Bool(r.u8()? != 0),
        t => return Err(Error::BadValueType(t)),
    })
}

fn write_value(w: &mut Writer, v: &Value) {
    w.u8(v.tag());
    match v {
        Value::None => {}
        Value::Identifier(i) | Value::String(i) => w.u16(*i),
        Value::Int(i) => w.u32(*i as u32),
        Value::Float(x) => w.u32(x.to_bits()),
        Value::Bool(b) => w.u8(*b as u8),
    }
}

fn read_function(r: &mut Reader<'_>, name: StrIndex, flavour: Flavour) -> Result<Function, Error> {
    let return_type = r.u16()?;
    let doc = r.u16()?;
    let user_flags = r.u32()?;
    let flags = r.u8()?;
    let n = r.u16()? as usize;
    let mut params = Vec::with_capacity(n);
    for _ in 0..n {
        params.push(TypedName {
            name: r.u16()?,
            type_name: r.u16()?,
        });
    }
    let n = r.u16()? as usize;
    let mut locals = Vec::with_capacity(n);
    for _ in 0..n {
        locals.push(TypedName {
            name: r.u16()?,
            type_name: r.u16()?,
        });
    }
    let n = r.u16()? as usize;
    let mut instructions = Vec::with_capacity(n);
    for _ in 0..n {
        let opcode = r.u8()?;
        let info = opcode_info(opcode, flavour).ok_or(Error::BadOpcode(opcode))?;
        let mut args = Vec::with_capacity(info.args as usize);
        for _ in 0..info.args {
            args.push(read_value(r)?);
        }
        let mut varargs = Vec::new();
        if info.varargs {
            let count = read_value(r)?;
            let Value::Int(n) = count else {
                return Err(Error::Layout(format!(
                    "{}'s vararg count is {count:?}, not an int",
                    info.name
                )));
            };
            if n < 0 {
                return Err(Error::Layout(format!(
                    "{}'s vararg count is negative ({n})",
                    info.name
                )));
            }
            for _ in 0..n {
                varargs.push(read_value(r)?);
            }
        }
        instructions.push(Instruction {
            opcode,
            args,
            varargs,
        });
    }
    Ok(Function {
        name,
        return_type,
        doc,
        user_flags,
        flags,
        params,
        locals,
        instructions,
    })
}

/// The opcode's shape, in the dialect the file is written in.
///
/// A Skyrim file answers from Skyrim's 36 opcodes alone: a Fallout 4 opcode in
/// a Skyrim stream is something the game's own loader would refuse, so reading
/// it as a zero-operand instruction would desync the rest of the function
/// rather than report the fault.
fn opcode_info(opcode: u8, flavour: Flavour) -> Option<crate::opcodes::OpInfo> {
    match flavour {
        Flavour::Skyrim => crate::opcodes::info(opcode),
        _ => crate::opcodes::info_any(opcode),
    }
}

/// A function body, without its name.
///
/// The name is not part of the body: a state's function list carries it before
/// each body, and a property accessor takes the property's own name. Writing
/// it here as well put two bytes in front of every accessor body — which is
/// what made 17 of the 14,026 shipped scripts fail to round-trip.
fn write_function(w: &mut Writer, f: &Function) -> Result<(), Error> {
    w.u16(f.return_type);
    w.u16(f.doc);
    w.u32(f.user_flags);
    w.u8(f.flags);
    w.u16(f.params.len() as u16);
    for p in &f.params {
        w.u16(p.name);
        w.u16(p.type_name);
    }
    w.u16(f.locals.len() as u16);
    for l in &f.locals {
        w.u16(l.name);
        w.u16(l.type_name);
    }
    w.u16(f.instructions.len() as u16);
    for i in &f.instructions {
        let info = crate::opcodes::info_any(i.opcode).ok_or(Error::BadOpcode(i.opcode))?;
        w.u8(i.opcode);
        for a in &i.args {
            write_value(w, a);
        }
        if info.varargs {
            // The count is a tagged int, not a bare u16: measured on
            // `Activate(akActivator, true)`, whose two-operand list is
            // preceded by `03 00 00 00 02`.
            write_value(w, &Value::Int(i.varargs.len() as i32));
            for a in &i.varargs {
                write_value(w, a);
            }
        } else if !i.varargs.is_empty() {
            return Err(Error::Layout(format!(
                "{} takes no vararg list, but this instruction has {} entries",
                info.name,
                i.varargs.len()
            )));
        }
    }
    Ok(())
}

fn read_object(r: &mut Reader<'_>, flavour: Flavour) -> Result<Object, Error> {
    let name = r.u16()?;
    let size = r.u32()?;
    let parent = r.u16()?;
    let doc = r.u16()?;
    let const_flag = if flavour.has_fo4_fields() {
        r.u8()? != 0
    } else {
        false
    };
    let user_flags = r.u32()?;
    let auto_state = r.u16()?;

    let mut structs = Vec::new();
    if flavour.has_fo4_fields() {
        let n = r.u16()? as usize;
        for _ in 0..n {
            let name = r.u16()?;
            let m = r.u16()? as usize;
            let mut members = Vec::with_capacity(m);
            for _ in 0..m {
                members.push(StructMember {
                    name: r.u16()?,
                    type_name: r.u16()?,
                    user_flags: r.u32()?,
                    initial_value: read_value(r)?,
                    const_flag: r.u8()? != 0,
                    doc: r.u16()?,
                });
            }
            structs.push(StructInfo { name, members });
        }
    }

    let n = r.u16()? as usize;
    let mut variables = Vec::with_capacity(n);
    for _ in 0..n {
        let name = r.u16()?;
        let type_name = r.u16()?;
        let user_flags = r.u32()?;
        let initial_value = read_value(r)?;
        let const_flag = if flavour.has_fo4_fields() {
            r.u8()? != 0
        } else {
            false
        };
        variables.push(Variable {
            name,
            type_name,
            user_flags,
            initial_value,
            const_flag,
        });
    }

    let n = r.u16()? as usize;
    let mut properties = Vec::with_capacity(n);
    for _ in 0..n {
        let name = r.u16()?;
        let type_name = r.u16()?;
        let doc = r.u16()?;
        let user_flags = r.u32()?;
        let flags = r.u8()?;
        let mut p = Property {
            name,
            type_name,
            doc,
            user_flags,
            flags,
            auto_var: None,
            getter: None,
            setter: None,
        };
        if p.is_auto() {
            p.auto_var = Some(r.u16()?);
        } else {
            if p.is_readable() {
                p.getter = Some(read_function(r, name, flavour)?);
            }
            if p.is_writable() {
                p.setter = Some(read_function(r, name, flavour)?);
            }
        }
        properties.push(p);
    }

    let mut guards = Vec::new();
    if flavour.has_guards() {
        let n = r.u16()? as usize;
        for _ in 0..n {
            guards.push(r.u16()?);
        }
    }

    let n = r.u16()? as usize;
    let mut states = Vec::with_capacity(n);
    for _ in 0..n {
        let name = r.u16()?;
        let m = r.u16()? as usize;
        let mut functions = Vec::with_capacity(m);
        for _ in 0..m {
            let fname = r.u16()?;
            functions.push(read_function(r, fname, flavour)?);
        }
        states.push(State { name, functions });
    }

    Ok(Object {
        name,
        size,
        parent,
        doc,
        const_flag,
        user_flags,
        auto_state,
        structs,
        variables,
        properties,
        guards,
        states,
    })
}

fn write_object(w: &mut Writer, f: &PexFile, o: &Object) -> Result<(), Error> {
    w.u16(o.name);
    w.u32(o.size);
    write_object_body(w, f.flavour, o);
    Ok(())
}

/// Everything after the `size` field. Split out so [`Object::computed_size`]
/// can measure the same bytes the writer emits rather than re-counting them.
fn write_object_body(w: &mut Writer, flavour: Flavour, o: &Object) {
    w.u16(o.parent);
    w.u16(o.doc);
    if flavour.has_fo4_fields() {
        w.u8(o.const_flag as u8);
    }
    w.u32(o.user_flags);
    w.u16(o.auto_state);

    if flavour.has_fo4_fields() {
        w.u16(o.structs.len() as u16);
        for s in &o.structs {
            w.u16(s.name);
            w.u16(s.members.len() as u16);
            for m in &s.members {
                w.u16(m.name);
                w.u16(m.type_name);
                w.u32(m.user_flags);
                write_value(w, &m.initial_value);
                w.u8(m.const_flag as u8);
                w.u16(m.doc);
            }
        }
    }

    w.u16(o.variables.len() as u16);
    for v in &o.variables {
        w.u16(v.name);
        w.u16(v.type_name);
        w.u32(v.user_flags);
        write_value(w, &v.initial_value);
        if flavour.has_fo4_fields() {
            w.u8(v.const_flag as u8);
        }
    }

    w.u16(o.properties.len() as u16);
    for p in &o.properties {
        w.u16(p.name);
        w.u16(p.type_name);
        w.u16(p.doc);
        w.u32(p.user_flags);
        w.u8(p.flags);
        if p.is_auto() {
            // A missing backing variable writes index 0 rather than failing:
            // this path is only reached when measuring, and `write` checks it.
            w.u16(p.auto_var.unwrap_or(0));
        } else {
            if let Some(g) = &p.getter {
                let _ = write_function(w, g);
            }
            if let Some(s) = &p.setter {
                let _ = write_function(w, s);
            }
        }
    }

    if flavour.has_guards() {
        w.u16(o.guards.len() as u16);
        for g in &o.guards {
            w.u16(*g);
        }
    }

    w.u16(o.states.len() as u16);
    for s in &o.states {
        w.u16(s.name);
        w.u16(s.functions.len() as u16);
        for fun in &s.functions {
            w.u16(fun.name);
            let _ = write_function(w, fun);
        }
    }
}
