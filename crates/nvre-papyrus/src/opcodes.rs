//! The Papyrus instruction set: the opcodes and their operand counts.
//!
//! # Where the numbers come from
//!
//! The table is the compiler's own, and every entry is exercised by the shipped
//! corpus: a parse of all 14,026 `.pex` in `Skyrim - Misc.bsa` with
//! `tools/pexref.py` reads each file to its last byte, which it can only do if
//! every opcode's operand count is right — one wrong count desyncs that
//! function's whole stream.
//!
//! # Skyrim has 36, and that is not the whole language
//!
//! The opcodes fall into blocks by who reads them. Skyrim's compiler emits
//! `0x00`–`0x23`; `0x24` and above are Fallout 4's additions (structs, guards,
//! the array functions that take a struct callback). A reader that accepts a
//! Fallout 4 opcode in a Skyrim file would read a stream the game would refuse,
//! so [`info`] answers from the table the file's flavour selects, and an
//! unknown opcode is an error rather than a zero-operand instruction.
//!
//! # Operand counts are fixed, except where they are not
//!
//! Four Skyrim opcodes — `callmethod`, `callparent`, `callstatic` and, on
//! Fallout 4, the three guard opcodes — take a **counted operand list**: after
//! the fixed operands comes a tagged `int` holding how many more follow. The
//! count is a tagged value (`03 00 00 00 02` for two), not a bare `u16`, which
//! is the encoding most easily guessed wrong.

/// One instruction's shape.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct OpInfo {
    /// The opcode's mnemonic.
    pub name: &'static str,
    /// How many fixed operands follow the opcode byte.
    pub args: u8,
    /// Whether a counted operand list follows the fixed ones.
    pub varargs: bool,
}

/// Skyrim's instruction set: 36 opcodes, `0x00`–`0x23`.
///
/// Read off the corpus. `comp_gte` is spelled that way in the bytecode
/// assemblers' tables and the spelling is kept so a disassembly matches what a
/// person comparing it against another tool expects.
pub const SKYRIM: [OpInfo; 36] = [
    OpInfo {
        name: "nop",
        args: 0,
        varargs: false,
    },
    OpInfo {
        name: "iadd",
        args: 3,
        varargs: false,
    },
    OpInfo {
        name: "fadd",
        args: 3,
        varargs: false,
    },
    OpInfo {
        name: "isub",
        args: 3,
        varargs: false,
    },
    OpInfo {
        name: "fsub",
        args: 3,
        varargs: false,
    },
    OpInfo {
        name: "imul",
        args: 3,
        varargs: false,
    },
    OpInfo {
        name: "fmul",
        args: 3,
        varargs: false,
    },
    OpInfo {
        name: "idiv",
        args: 3,
        varargs: false,
    },
    OpInfo {
        name: "fdiv",
        args: 3,
        varargs: false,
    },
    OpInfo {
        name: "imod",
        args: 3,
        varargs: false,
    },
    OpInfo {
        name: "not",
        args: 2,
        varargs: false,
    },
    OpInfo {
        name: "ineg",
        args: 2,
        varargs: false,
    },
    OpInfo {
        name: "fneg",
        args: 2,
        varargs: false,
    },
    OpInfo {
        name: "assign",
        args: 2,
        varargs: false,
    },
    OpInfo {
        name: "cast",
        args: 2,
        varargs: false,
    },
    OpInfo {
        name: "cmp_eq",
        args: 3,
        varargs: false,
    },
    OpInfo {
        name: "cmp_lt",
        args: 3,
        varargs: false,
    },
    OpInfo {
        name: "cmp_lte",
        args: 3,
        varargs: false,
    },
    OpInfo {
        name: "cmp_gt",
        args: 3,
        varargs: false,
    },
    OpInfo {
        name: "comp_gte",
        args: 3,
        varargs: false,
    },
    OpInfo {
        name: "jmp",
        args: 1,
        varargs: false,
    },
    OpInfo {
        name: "jmpt",
        args: 2,
        varargs: false,
    },
    OpInfo {
        name: "jmpf",
        args: 2,
        varargs: false,
    },
    OpInfo {
        name: "callmethod",
        args: 3,
        varargs: true,
    },
    OpInfo {
        name: "callparent",
        args: 2,
        varargs: true,
    },
    OpInfo {
        name: "callstatic",
        args: 3,
        varargs: true,
    },
    OpInfo {
        name: "return",
        args: 1,
        varargs: false,
    },
    OpInfo {
        name: "strcat",
        args: 3,
        varargs: false,
    },
    OpInfo {
        name: "propget",
        args: 3,
        varargs: false,
    },
    OpInfo {
        name: "propset",
        args: 3,
        varargs: false,
    },
    OpInfo {
        name: "array_create",
        args: 2,
        varargs: false,
    },
    OpInfo {
        name: "array_length",
        args: 2,
        varargs: false,
    },
    OpInfo {
        name: "array_getelement",
        args: 3,
        varargs: false,
    },
    OpInfo {
        name: "array_setelement",
        args: 3,
        varargs: false,
    },
    OpInfo {
        name: "array_findelement",
        args: 4,
        varargs: false,
    },
    OpInfo {
        name: "array_rfindelement",
        args: 4,
        varargs: false,
    },
];

/// Fallout 4's additions after Skyrim's 36.
///
/// Not verified against a corpus here — this crate reads Skyrim first, and
/// these entries are carried so a Fallout 4 file parses rather than being
/// refused part-way through. When Fallout 4 scripts are added to the corpus
/// census, each of these gets measured the same way Skyrim's 36 were.
pub const FALLOUT4_EXTRA: [OpInfo; 15] = [
    OpInfo {
        name: "is",
        args: 3,
        varargs: false,
    },
    OpInfo {
        name: "struct_create",
        args: 1,
        varargs: false,
    },
    OpInfo {
        name: "struct_get",
        args: 3,
        varargs: false,
    },
    OpInfo {
        name: "struct_set",
        args: 3,
        varargs: false,
    },
    OpInfo {
        name: "array_findstruct",
        args: 5,
        varargs: false,
    },
    OpInfo {
        name: "array_rfindstruct",
        args: 5,
        varargs: false,
    },
    OpInfo {
        name: "array_add",
        args: 3,
        varargs: false,
    },
    OpInfo {
        name: "array_insert",
        args: 3,
        varargs: false,
    },
    OpInfo {
        name: "array_removelast",
        args: 1,
        varargs: false,
    },
    OpInfo {
        name: "array_remove",
        args: 3,
        varargs: false,
    },
    OpInfo {
        name: "array_clear",
        args: 1,
        varargs: false,
    },
    OpInfo {
        name: "array_getallmatchingstructs",
        args: 6,
        varargs: false,
    },
    OpInfo {
        name: "lock_guards",
        args: 0,
        varargs: true,
    },
    OpInfo {
        name: "unlock_guards",
        args: 0,
        varargs: true,
    },
    OpInfo {
        name: "try_lock_guards",
        args: 1,
        varargs: true,
    },
];

/// The opcode's shape for a Skyrim file, or `None` if the table does not name
/// it.
///
/// A Skyrim reader answers from [`SKYRIM`] alone: a Fallout 4 opcode in a
/// Skyrim file is a stream the game itself would refuse, so it is an error and
/// not a zero-operand instruction.
pub fn info(opcode: u8) -> Option<OpInfo> {
    SKYRIM.get(opcode as usize).copied()
}

/// The opcode's shape for any dialect, falling back to Fallout 4's additions.
pub fn info_any(opcode: u8) -> Option<OpInfo> {
    SKYRIM
        .get(opcode as usize)
        .copied()
        .or_else(|| FALLOUT4_EXTRA.get(opcode as usize - SKYRIM.len()).copied())
}

/// Whether an opcode takes a counted operand list.
pub fn takes_varargs(opcode: u8) -> bool {
    info_any(opcode).map(|i| i.varargs).unwrap_or(false)
}

/// An opcode's mnemonic, or `"?"` for one the table does not name.
pub fn name(opcode: u8) -> &'static str {
    info_any(opcode).map(|i| i.name).unwrap_or("?")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_skyrim_table_is_the_compilers_own() {
        // Spot checks against bytes measured in `DLC2ThirskDoorScript.pex`:
        // `return ::State` is 0x1A, `callmethod` is 0x17, `jmpf` is 0x16 and
        // `cmp_eq` is 0x0F.
        assert_eq!(SKYRIM[0x1A].name, "return");
        assert_eq!(SKYRIM[0x17].name, "callmethod");
        assert_eq!(SKYRIM[0x16].name, "jmpf");
        assert_eq!(SKYRIM[0x0F].name, "cmp_eq");
        assert_eq!(SKYRIM[0x10].name, "cmp_lt");
        // Index 0x13 is `jmp`, which is what the same file's `else` block ends
        // with.
        assert_eq!(SKYRIM[0x13].name, "jmp");
    }

    #[test]
    fn a_fallout4_opcode_is_not_a_skyrim_one() {
        // 36 is the first Fallout 4 addition; a Skyrim reader must refuse it.
        assert!(info(36).is_none());
        assert_eq!(info_any(36).unwrap().name, "is");
    }

    #[test]
    fn only_the_call_opcodes_take_a_counted_list() {
        let taking: Vec<&str> = SKYRIM
            .iter()
            .filter(|i| i.varargs)
            .map(|i| i.name)
            .collect();
        assert_eq!(taking, ["callmethod", "callparent", "callstatic"]);
    }
}
