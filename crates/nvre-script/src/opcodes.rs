//! The obscript opcodes: the statement and command instruction codes the
//! game's bytecode uses.
//!
//! Every instruction in a `SCDA` body is `[opcode:u16][argLen:u16][args…]`,
//! little-endian. An opcode below `0x1000` is a **statement** — the language's
//! own control flow — and one at `0x1000` or above is a **command**: a call to
//! one of the game's built-in functions.
//!
//! # Why the table is data and not a match
//!
//! The command set is large — 226 distinct opcodes appear in
//! `FalloutNV.esm`'s scripts alone, out of roughly 533 the base game
//! registers — and each one needs its parameter encoding as well as its name.
//! A `match` over 226 arms in the emitter would be 226 places to get an
//! encoding wrong, so the table lives here as data that the emitter walks.
//!
//! # Where the numbers come from
//!
//! The statement opcodes are read straight off the game's own bytes, and every
//! one of them appears in the shipped corpus. The command opcodes are paired
//! with their names by the corpus itself: each `SCPT` carries both its source
//! and its compiled bytes, so the name a source spells and the opcode its
//! bytes use are two views of the same call site. `examples/obscript_names.rs`
//! prints that pairing, and what it settles with a clear majority is what this
//! table holds. An entry the corpus has not settled is absent here rather than
//! guessed: a wrong opcode compiles to a call to the wrong function, which the
//! game will happily run.

/// The statement opcodes, `0x10`–`0x1F`.
///
/// These are the language's own control flow, and every one of them is
/// observed in the shipped corpus. `0x1A` is the one exception: it appears
/// exactly once in `FalloutNV.esm` and the spec does not name it, so it is
/// listed as unknown rather than assigned a keyword the parser would emit.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Statement {
    /// `Begin <block> …` — opens an event block.
    Begin,
    /// `End` — closes the block `Begin` opened.
    End,
    /// `short x` — declares an integer local.
    Short,
    /// `long x` — declares an integer local.
    Long,
    /// `float x` — declares a float local.
    Float,
    /// `ref x` — declares a reference local.
    Ref,
    /// `set x to <expr>` — assigns.
    Set,
    /// `if <expr>` — opens a conditional.
    If,
    /// `else`
    Else,
    /// `elseif <expr>`
    ElseIf,
    /// `endif` — the end of the chain. Not emitted as its own instruction:
    /// the `If`'s skip count jumps past it.
    EndIf,
    /// `return`
    Return,
    /// `ScriptName <name>` — the script's own name, first in every `SCPT`.
    ScriptName,
    /// `SomeRef.` — the next command applies to this reference.
    SetCurrentRef,
    /// An opcode in the statement range the spec does not name.
    Unknown(u16),
}

impl Statement {
    /// The statement this opcode is, if it is one.
    pub fn from_opcode(op: u16) -> Option<Self> {
        use Statement::*;
        if !Self::is_statement(op) {
            return None;
        }
        Some(match op {
            0x10 => Begin,
            0x11 => End,
            0x12 => Short,
            0x13 => Long,
            0x14 => Float,
            0x1F => Ref,
            0x15 => Set,
            0x16 => If,
            0x17 => Else,
            0x18 => ElseIf,
            0x19 => EndIf,
            0x1E => Return,
            0x1D => ScriptName,
            0x1C => SetCurrentRef,
            // Anything else in the range is a statement the spec does not
            // name. It is carried as itself rather than dropped, so a body
            // that uses it survives a decode.
            _ => Unknown(op),
        })
    }

    /// The opcode this statement is written as.
    pub fn opcode(self) -> u16 {
        use Statement::*;
        match self {
            Begin => 0x10,
            End => 0x11,
            Short => 0x12,
            Long => 0x13,
            Float => 0x14,
            Ref => 0x1F,
            Set => 0x15,
            If => 0x16,
            Else => 0x17,
            ElseIf => 0x18,
            EndIf => 0x19,
            Return => 0x1E,
            ScriptName => 0x1D,
            SetCurrentRef => 0x1C,
            Unknown(op) => op,
        }
    }

    /// The keyword this statement is written as in source, empty for the ones
    /// that emit no keyword of their own.
    pub fn keyword(self) -> &'static str {
        use Statement::*;
        match self {
            Begin => "Begin",
            End => "End",
            Short => "short",
            Long => "long",
            Float => "float",
            Ref => "ref",
            Set => "set",
            If => "if",
            Else => "else",
            ElseIf => "elseif",
            EndIf => "endif",
            Return => "return",
            ScriptName => "ScriptName",
            SetCurrentRef => "",
            Unknown(_) => "",
        }
    }

    /// Whether this opcode is in the statement range at all.
    pub fn is_statement(op: u16) -> bool {
        (0x10..=0x1F).contains(&op)
    }
}

/// One command: its name, its opcode, and the arguments it takes.
///
/// The argument list is what completion inserts and what the emitter reads to
/// know how many operands a call consumes. The names are the ones the GECK's
/// own documentation uses; the count is what the shipped bytecode shows.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Command {
    /// The command's name, as a script spells it. Case-insensitive in source.
    pub name: &'static str,
    /// The opcode the game's compiler gives it.
    pub opcode: u16,
    /// The arguments it takes, in order. Empty means it takes none.
    pub args: &'static [&'static str],
}

/// The command table: the entries the corpus settled with a clear majority.
///
/// Each row was derived by compiling a shipped unit and reading the opcode its
/// bytes carry for that call — see `examples/obscript_names.rs`, which prints
/// the vote behind every entry. A name the corpus did not settle is absent,
/// because a guessed opcode is a call to the wrong function.
pub const COMMANDS: &[Command] = &[
    Command {
        // Measured from the corpus: `set userref to getactionref` in
        // vCCCrucifiedSlaveSCRIPT compiles to `20 58 CD 10 00 00`, so the `X`
        // token, opcode 0x10CD, and a zero parameter length.
        name: "GetActionRef",
        opcode: 0x10CD,
        args: &[],
    },
    Command {
        name: "Activate",
        opcode: 0x100D,
        args: &["ObjectRefID", "Flag"],
    },
    Command {
        name: "AddAchievement",
        opcode: 0x114A,
        args: &["Achievement"],
    },
    Command {
        name: "AddItem",
        opcode: 0x1002,
        args: &["ObjectID", "Count", "Flag"],
    },
    Command {
        name: "AddPerk",
        opcode: 0x1176,
        args: &["Perk"],
    },
    Command {
        name: "AddReputation",
        opcode: 0x1239,
        args: &["Reputation", "Value"],
    },
    Command {
        name: "AddScriptPackage",
        opcode: 0x1097,
        args: &["Package"],
    },
    Command {
        name: "AddSpell",
        opcode: 0x101C,
        args: &["Spell"],
    },
    Command {
        name: "ApplyImageSpaceModifier",
        opcode: 0x1191,
        args: &["ImageSpaceModifier", "Flag"],
    },
    Command {
        name: "AttachAshPile",
        opcode: 0x1211,
        args: &[],
    },
    Command {
        name: "BWaitersServeWine",
        opcode: 0x11C8,
        args: &[],
    },
    Command {
        name: "ClearFactionPlayerEnemyFlag",
        opcode: 0x11C9,
        args: &["Faction"],
    },
    Command {
        name: "CompleteQuest",
        opcode: 0x1071,
        args: &["Quest"],
    },
    Command {
        name: "DamageAV",
        opcode: 0x1181,
        args: &["ActorValue", "Amount"],
    },
    Command {
        name: "Dispel",
        opcode: 0x1142,
        args: &[],
    },
    Command {
        name: "DropMe",
        opcode: 0x10A6,
        args: &[],
    },
    Command {
        name: "EnableFastTravel",
        opcode: 0x1111,
        args: &["Flag"],
    },
    Command {
        name: "EnableNavMesh",
        opcode: 0x11C0,
        args: &["Flag"],
    },
    Command {
        name: "GetAV",
        opcode: 0x11CD,
        args: &["ActorValue"],
    },
    Command {
        // `Kill` is the same command under a shorter name: the corpus spells
        // it both ways (`Kill;`, `KillActor`, `killActor Opp 1 2`) and both
        // compile to 0x108B. One entry, because two rows with one opcode is a
        // table that cannot be looked up.
        name: "KillActor",
        opcode: 0x108B,
        args: &["Target", "Flag"],
    },
    Command {
        name: "PlaceAtMe",
        opcode: 0x1025,
        args: &["ObjectID", "Count", "Distance", "Direction"],
    },
    Command {
        name: "RemoveSpell",
        opcode: 0x101D,
        args: &["Spell"],
    },
    Command {
        name: "RestoreAV",
        opcode: 0x1182,
        args: &["ActorValue", "Amount"],
    },
    Command {
        name: "SetAngle",
        opcode: 0x1009,
        args: &["Axis", "Angle"],
    },
    Command {
        name: "SetAV",
        opcode: 0x100F,
        args: &["ActorValue", "Value"],
    },
    Command {
        name: "ShowMessage",
        opcode: 0x1059,
        args: &["Message"],
    },
    Command {
        // Parameterless: the game writes no argument bytes at all for it.
        name: "TrapUpdate",
        opcode: 0x110C,
        args: &[],
    },
    Command {
        // Parameterless: the game writes no argument bytes at all for it.
        name: "SCAOnActor",
        opcode: 0x1101,
        args: &[],
    },
    Command {
        // Parameterless: the game writes no argument bytes at all for it.
        name: "EVP",
        opcode: 0x105E,
        args: &[],
    },
    Command {
        // Parameterless: the game writes no argument bytes at all for it.
        name: "PurgeCellBuffers",
        opcode: 0x1189,
        args: &[],
    },
    Command {
        // Parameterless: the game writes no argument bytes at all for it.
        name: "MarkForDelete",
        opcode: 0x11BB,
        args: &[],
    },
    Command {
        // Parameterless: the game writes no argument bytes at all for it.
        name: "ExitGame",
        opcode: 0x11B5,
        args: &[],
    },
    Command {
        // Parameterless: the game writes no argument bytes at all for it.
        name: "ReleaseWeatherOverride",
        opcode: 0x1158,
        args: &[],
    },
    Command {
        // Parameterless: the game writes no argument bytes at all for it.
        name: "ResetHealth",
        opcode: 0x1150,
        args: &[],
    },
    Command {
        name: "AddTopic",
        opcode: 0x1058,
        args: &["Topic"],
    },
    Command {
        // Declares a parameter, even though `Disable` is normally called with
        // none: the game writes `22 10 02 00 00 00` for it, a count of zero
        // rather than no argument bytes at all. Measured against
        // vLegionaryFactionRewardNPCScript.
        name: "Disable",
        opcode: 0x1022,
        args: &["Flag"],
    },
    Command {
        name: "Enable",
        opcode: 0x1021,
        args: &["Flag"],
    },
    Command {
        name: "EquipItem",
        opcode: 0x10EE,
        args: &["ObjectID", "Flag"],
    },
    Command {
        name: "IgnoreCrime",
        opcode: 0x10AB,
        args: &["Flag"],
    },
    Command {
        name: "Lock",
        opcode: 0x1072,
        args: &["LockLevel"],
    },
    Command {
        name: "Look",
        opcode: 0x105C,
        args: &["Target"],
    },
    Command {
        name: "ModActorValue",
        opcode: 0x1010,
        args: &["ActorValue", "Amount"],
    },
    Command {
        name: "MoveTo",
        opcode: 0x109E,
        args: &["Marker"],
    },
    Command {
        name: "PMS",
        opcode: 0x1123,
        args: &["Shader"],
    },
    Command {
        name: "PickIdle",
        opcode: 0x1064,
        args: &[],
    },
    Command {
        name: "PlayGroup",
        opcode: 0x1013,
        args: &["AnimGroup", "Flag"],
    },
    Command {
        name: "PlaySound",
        opcode: 0x1026,
        args: &["Sound"],
    },
    Command {
        name: "PlaySound3D",
        opcode: 0x10B2,
        args: &["Sound"],
    },
    Command {
        name: "RemoveAllItems",
        opcode: 0x10AD,
        args: &["Target", "Flag"],
    },
    Command {
        name: "RemoveItem",
        opcode: 0x1052,
        args: &["ObjectID", "Count"],
    },
    Command {
        // Declares a parameter, even though `RemoveMe` is normally called
        // with none: the game writes `A5 10 02 00 00 00` for it, which is a
        // count of zero rather than no argument bytes at all. Measured against
        // L38upgradeDisplaycase02SCRIPT.
        name: "RemoveMe",
        opcode: 0x10A5,
        args: &["Flag"],
    },
    Command {
        // Declares a parameter: `Resurrect` is `8C 10 02 00 00 00`.
        name: "Resurrect",
        opcode: 0x108C,
        args: &["Flag"],
    },
    Command {
        name: "RewardXP",
        opcode: 0x1149,
        args: &["XP"],
    },
    Command {
        name: "SMS",
        opcode: 0x1125,
        args: &["Shader"],
    },
    Command {
        name: "Say",
        opcode: 0x1033,
        args: &["Topic", "Flag"],
    },
    Command {
        name: "SayTo",
        opcode: 0x1034,
        args: &["Actor", "Topic"],
    },
    Command {
        name: "SetAlly",
        opcode: 0x1079,
        args: &["Faction1", "Faction2", "Flag"],
    },
    Command {
        name: "SetCombatStyle",
        opcode: 0x10B1,
        args: &["CombatStyle"],
    },
    Command {
        name: "SetCriticalStage",
        opcode: 0x1148,
        args: &["Stage"],
    },
    Command {
        name: "SetDestroyed",
        opcode: 0x10CC,
        args: &["Flag"],
    },
    Command {
        name: "SetEnemy",
        opcode: 0x1078,
        args: &["Faction1", "Faction2"],
    },
    Command {
        name: "SetGhost",
        opcode: 0x10EC,
        args: &["Flag"],
    },
    Command {
        name: "SetOpenState",
        opcode: 0x10DD,
        args: &["OpenState"],
    },
    Command {
        name: "SetRestrained",
        opcode: 0x10F3,
        args: &["Flag"],
    },
    Command {
        name: "SetStage",
        opcode: 0x1039,
        args: &["Quest", "Stage"],
    },
    Command {
        name: "SetUnconscious",
        opcode: 0x10F1,
        args: &["Flag"],
    },
    Command {
        name: "SetWeather",
        opcode: 0x1104,
        args: &["Weather", "Flag"],
    },
    Command {
        name: "StartCombat",
        opcode: 0x1016,
        args: &["Target"],
    },
    Command {
        name: "StartConversation",
        opcode: 0x1056,
        args: &["Actor", "Topic"],
    },
    Command {
        name: "StartQuest",
        opcode: 0x1036,
        args: &["Quest"],
    },
    Command {
        name: "StopCombat",
        opcode: 0x1017,
        args: &[],
    },
    Command {
        name: "StopQuest",
        opcode: 0x1037,
        args: &["Quest"],
    },
    Command {
        name: "UnequipItem",
        opcode: 0x10EF,
        args: &["ObjectID", "Flag"],
    },
    Command {
        name: "Unlock",
        opcode: 0x1073,
        // The game's command definition declares an optional parameter, so
        // calling it without parameters emits a 2-byte count of 0 (00 00).
        args: &["Optional"],
    },
];

/// The command with this name, case-insensitively.
pub fn command_named(name: &str) -> Option<&'static Command> {
    COMMANDS.iter().find(|c| c.name.eq_ignore_ascii_case(name))
}

/// The aliases the corpus spells a command under.
///
/// A name the writers use for a command whose table entry is called something
/// else. These are recorded rather than added as second rows with the same
/// opcode, because two rows sharing an opcode is a table that cannot be
/// looked up — and the corpus spells `Kill` and `KillActor` for the same
/// 0x108B.
const ALIASES: &[(&str, &str)] = &[("Kill", "KillActor"), ("SetActorValue", "SetAV")];

/// The command called `name`, resolving an alias first.
pub fn command_called(name: &str) -> Option<&'static Command> {
    if let Some(c) = command_named(name) {
        return Some(c);
    }
    let canonical = ALIASES
        .iter()
        .find(|(alias, _)| alias.eq_ignore_ascii_case(name))
        .map(|(_, canonical)| *canonical)?;
    command_named(canonical)
}

/// The command with this opcode.
pub fn command_at(opcode: u16) -> Option<&'static Command> {
    COMMANDS.iter().find(|c| c.opcode == opcode)
}

/// Every command whose name starts with `prefix`, case-insensitively.
pub fn commands_starting_with(prefix: &str) -> Vec<&'static Command> {
    let prefix = prefix.to_ascii_lowercase();
    let mut out: Vec<&Command> = COMMANDS
        .iter()
        .filter(|c| c.name.to_ascii_lowercase().starts_with(&prefix))
        .collect();
    out.sort_by_key(|c| c.name);
    out
}

/// The `Begin` block event codes: which event a block handles.
///
/// A `Begin`'s third argument is the event, and the compiler writes it as a
/// bare u16. The names are what the GECK's own documentation spells, so a
/// decompiled script says `Begin GameMode` rather than `Begin 0`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Event {
    GameMode,
    MenuMode,
    OnActivate,
    OnAdd,
    OnEquip,
    OnUnequip,
    OnDrop,
    SayToDone,
    OnHit,
    OnHitWith,
    OnDeath,
    OnMurder,
    OnCombatEnd,
    OnPackageStart,
    OnPackageDone,
    ScriptEffectStart,
    ScriptEffectFinish,
    ScriptEffectUpdate,
    OnPackageChange,
    OnLoad,
    OnMagicEffectHit,
    OnSell,
    OnTrigger,
    OnStartCombat,
    OnTriggerEnter,
    OnTriggerLeave,
    OnActorEquip,
    OnActorUnequip,
    OnReset,
    OnOpen,
    OnClose,
    OnGrab,
    OnRelease,
    OnDestructionStageChange,
    OnFire,
}

impl Event {
    /// The event this code is, if it is one.
    /// Whether this event's block can take a parameter.
    ///
    /// The game writes a `paramCount` field in a `Begin`'s arguments only for
    /// the events that can receive one, and the corpus decides which without
    /// ambiguity: 1,132 parameterless `Begin`s of the no-argument events write
    /// six argument bytes with no count at all, and 1,033 of the others write
    /// eight with a count of zero. `GameMode` is six; `OnActivate` is eight
    /// even when nothing is passed to it.
    pub fn takes_argument(self) -> bool {
        !matches!(
            self,
            Self::GameMode
                | Self::OnCombatEnd
                | Self::ScriptEffectStart
                | Self::ScriptEffectFinish
                | Self::ScriptEffectUpdate
                | Self::OnLoad
                | Self::OnReset
                | Self::OnOpen
                | Self::OnClose
                | Self::OnGrab
                | Self::OnRelease
                | Self::OnFire
        )
    }

    pub fn from_code(code: u16) -> Option<Self> {
        use Event::*;
        Some(match code {
            0x0000 => GameMode,
            0x0001 => MenuMode,
            0x0002 => OnActivate,
            0x0003 => OnAdd,
            0x0004 => OnEquip,
            0x0005 => OnUnequip,
            0x0006 => OnDrop,
            0x0007 => SayToDone,
            0x0008 => OnHit,
            0x0009 => OnHitWith,
            0x000A => OnDeath,
            0x000B => OnMurder,
            0x000C => OnCombatEnd,
            0x000F => OnPackageStart,
            0x0010 => OnPackageDone,
            0x0011 => ScriptEffectStart,
            0x0012 => ScriptEffectFinish,
            0x0013 => ScriptEffectUpdate,
            0x0014 => OnPackageChange,
            0x0015 => OnLoad,
            0x0016 => OnMagicEffectHit,
            0x0017 => OnSell,
            0x0018 => OnTrigger,
            0x0019 => OnStartCombat,
            0x001A => OnTriggerEnter,
            0x001B => OnTriggerLeave,
            0x001C => OnActorEquip,
            0x001D => OnActorUnequip,
            0x001E => OnReset,
            0x001F => OnOpen,
            0x0020 => OnClose,
            0x0021 => OnGrab,
            0x0022 => OnRelease,
            0x0023 => OnDestructionStageChange,
            0x0024 => OnFire,
            _ => return None,
        })
    }

    /// The code this event is written as.
    pub fn code(self) -> u16 {
        use Event::*;
        match self {
            GameMode => 0x0000,
            MenuMode => 0x0001,
            OnActivate => 0x0002,
            OnAdd => 0x0003,
            OnEquip => 0x0004,
            OnUnequip => 0x0005,
            OnDrop => 0x0006,
            SayToDone => 0x0007,
            OnHit => 0x0008,
            OnHitWith => 0x0009,
            OnDeath => 0x000A,
            OnMurder => 0x000B,
            OnCombatEnd => 0x000C,
            OnPackageStart => 0x000F,
            OnPackageDone => 0x0010,
            ScriptEffectStart => 0x0011,
            ScriptEffectFinish => 0x0012,
            ScriptEffectUpdate => 0x0013,
            OnPackageChange => 0x0014,
            OnLoad => 0x0015,
            OnMagicEffectHit => 0x0016,
            OnSell => 0x0017,
            OnTrigger => 0x0018,
            OnStartCombat => 0x0019,
            OnTriggerEnter => 0x001A,
            OnTriggerLeave => 0x001B,
            OnActorEquip => 0x001C,
            OnActorUnequip => 0x001D,
            OnReset => 0x001E,
            OnOpen => 0x001F,
            OnClose => 0x0020,
            OnGrab => 0x0021,
            OnRelease => 0x0022,
            OnDestructionStageChange => 0x0023,
            OnFire => 0x0024,
        }
    }

    /// The event's name, as a source spells it.
    pub fn name(self) -> &'static str {
        use Event::*;
        match self {
            GameMode => "GameMode",
            MenuMode => "MenuMode",
            OnActivate => "OnActivate",
            OnAdd => "OnAdd",
            OnEquip => "OnEquip",
            OnUnequip => "OnUnequip",
            OnDrop => "OnDrop",
            SayToDone => "SayToDone",
            OnHit => "OnHit",
            OnHitWith => "OnHitWith",
            OnDeath => "OnDeath",
            OnMurder => "OnMurder",
            OnCombatEnd => "OnCombatEnd",
            OnPackageStart => "OnPackageStart",
            OnPackageDone => "OnPackageDone",
            ScriptEffectStart => "ScriptEffectStart",
            ScriptEffectFinish => "ScriptEffectFinish",
            ScriptEffectUpdate => "ScriptEffectUpdate",
            OnPackageChange => "OnPackageChange",
            OnLoad => "OnLoad",
            OnMagicEffectHit => "OnMagicEffectHit",
            OnSell => "OnSell",
            OnTrigger => "OnTrigger",
            OnStartCombat => "OnStartCombat",
            OnTriggerEnter => "OnTriggerEnter",
            OnTriggerLeave => "OnTriggerLeave",
            OnActorEquip => "OnActorEquip",
            OnActorUnequip => "OnActorUnequip",
            OnReset => "OnReset",
            OnOpen => "OnOpen",
            OnClose => "OnClose",
            OnGrab => "OnGrab",
            OnRelease => "OnRelease",
            OnDestructionStageChange => "OnDestructionStageChange",
            OnFire => "OnFire",
        }
    }

    /// The event with this name, case-insensitively.
    pub fn from_name(name: &str) -> Option<Self> {
        use Event::*;
        Some(match name.to_ascii_lowercase().as_str() {
            "gamemode" => GameMode,
            "menumode" => MenuMode,
            "onactivate" => OnActivate,
            "onadd" => OnAdd,
            "onequip" => OnEquip,
            "onunequip" => OnUnequip,
            "ondrop" => OnDrop,
            "saytodone" => SayToDone,
            "onhit" => OnHit,
            "onhitwith" => OnHitWith,
            "ondeath" => OnDeath,
            "onmurder" => OnMurder,
            "oncombatend" => OnCombatEnd,
            "onpackagestart" => OnPackageStart,
            "onpackagedone" => OnPackageDone,
            // The GECK's own blocktype table names this one `OnPackageEnd`,
            // and the corpus spells it both ways: 12 units write
            // `OnPackageEnd` and 4 write `OnPackageDone`, and both compile to
            // 0x0010. So the name is an alias of the same event rather than a
            // separate one.
            "onpackageend" => OnPackageDone,
            "scripteffectstart" => ScriptEffectStart,
            "scripteffectfinish" => ScriptEffectFinish,
            "scripteffectupdate" => ScriptEffectUpdate,
            "onpackagechange" => OnPackageChange,
            "onload" => OnLoad,
            "onmagiceffecthit" => OnMagicEffectHit,
            "onsell" => OnSell,
            "ontrigger" => OnTrigger,
            "onstartcombat" => OnStartCombat,
            "ontriggerenter" => OnTriggerEnter,
            "ontriggerleave" => OnTriggerLeave,
            "onactorequip" => OnActorEquip,
            "onactorunequip" => OnActorUnequip,
            "onreset" => OnReset,
            "onopen" => OnOpen,
            "onclose" => OnClose,
            "ongrab" => OnGrab,
            "onrelease" => OnRelease,
            "ondestructionstagechange" => OnDestructionStageChange,
            "onfire" => OnFire,
            _ => return None,
        })
    }
}
