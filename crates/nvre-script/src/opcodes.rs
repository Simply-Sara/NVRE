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
        args: &["Integer"],
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
        args: &["ActorValue", "Float"],
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
        name: "GetContainerInventoryCount",
        opcode: 0x127A,
        args: &[],
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
        args: &["ActorValue", "Float"],
    },
    Command {
        name: "SetAngle",
        opcode: 0x1009,
        args: &["Axis", "Float"],
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
        args: &["ActorValue", "Float"],
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
        opcode: 0x1177,
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
        opcode: 0x1212,
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
    Command {
        name: "GetDistance",
        opcode: 0x1001,
        args: &["ObjectReferenceID"],
    },
    Command {
        name: "SetEssential",
        opcode: 0x1003,
        args: &["Actor Base", "Integer"],
    },
    Command {
        name: "Rotate",
        opcode: 0x1004,
        args: &["Axis", "Speed"],
    },
    Command {
        name: "GetLocked",
        opcode: 0x1005,
        args: &[],
    },
    Command {
        name: "GetPos",
        opcode: 0x1006,
        args: &["Axis"],
    },
    Command {
        name: "SetPos",
        opcode: 0x1007,
        args: &["Axis", "Float"],
    },
    Command {
        name: "GetAngle",
        opcode: 0x1008,
        args: &["Axis"],
    },
    Command {
        name: "GetStartingPos",
        opcode: 0x100A,
        args: &["Axis"],
    },
    Command {
        name: "GetStartingAngle",
        opcode: 0x100B,
        args: &["Axis"],
    },
    Command {
        name: "GetSecondsPassed",
        opcode: 0x100C,
        args: &[],
    },
    Command {
        name: "GetActorValue",
        opcode: 0x100E,
        args: &["Actor Value"],
    },
    Command {
        name: "SetAtStart",
        opcode: 0x1011,
        args: &[],
    },
    Command {
        name: "GetCurrentTime",
        opcode: 0x1012,
        args: &[],
    },
    Command {
        name: "LoopGroup",
        opcode: 0x1014,
        args: &["Animation Group", "Integer", "Flags"],
    },
    Command {
        name: "SkipAnim",
        opcode: 0x1015,
        args: &[],
    },
    Command {
        name: "GetScale",
        opcode: 0x1018,
        args: &[],
    },
    Command {
        name: "IsMoving",
        opcode: 0x1019,
        args: &[],
    },
    Command {
        name: "IsTurning",
        opcode: 0x101A,
        args: &[],
    },
    Command {
        name: "GetLineOfSight",
        opcode: 0x101B,
        args: &["ObjectReferenceID"],
    },
    Command {
        name: "Cast",
        opcode: 0x101E,
        args: &["Magic Item", "ObjectReferenceID"],
    },
    Command {
        name: "GetButtonPressed",
        opcode: 0x101F,
        args: &[],
    },
    Command {
        name: "GetInSameCell",
        opcode: 0x1020,
        args: &["ObjectReferenceID"],
    },
    Command {
        name: "GetDisabled",
        opcode: 0x1023,
        args: &[],
    },
    Command {
        name: "MenuMode",
        opcode: 0x1024,
        args: &["Integer"],
    },
    Command {
        name: "GetDisease",
        opcode: 0x1027,
        args: &[],
    },
    Command {
        name: "GetVampire",
        opcode: 0x1028,
        args: &[],
    },
    Command {
        name: "GetClothingValue",
        opcode: 0x1029,
        args: &[],
    },
    Command {
        name: "SameFaction",
        opcode: 0x102A,
        args: &["Actor"],
    },
    Command {
        name: "SameRace",
        opcode: 0x102B,
        args: &["Actor"],
    },
    Command {
        name: "SameSex",
        opcode: 0x102C,
        args: &["Actor"],
    },
    Command {
        name: "GetDetected",
        opcode: 0x102D,
        args: &["Actor"],
    },
    Command {
        name: "GetDead",
        opcode: 0x102E,
        args: &[],
    },
    Command {
        name: "GetItemCount",
        opcode: 0x102F,
        args: &["ObjectID"],
    },
    Command {
        name: "GetGold",
        opcode: 0x1030,
        args: &[],
    },
    Command {
        name: "GetSleeping",
        opcode: 0x1031,
        args: &[],
    },
    Command {
        name: "GetTalkedToPC",
        opcode: 0x1032,
        args: &[],
    },
    Command {
        name: "GetScriptVariable",
        opcode: 0x1035,
        args: &["ObjectReferenceID", "Variable Name"],
    },
    Command {
        name: "GetQuestRunning",
        opcode: 0x1038,
        args: &["Quest"],
    },
    Command {
        name: "GetStage",
        opcode: 0x103A,
        args: &["Quest"],
    },
    Command {
        name: "GetStageDone",
        opcode: 0x103B,
        args: &["Quest", "Stage"],
    },
    Command {
        name: "GetFactionRankDifference",
        opcode: 0x103C,
        args: &["Faction", "Actor"],
    },
    Command {
        name: "GetAlarmed",
        opcode: 0x103D,
        args: &[],
    },
    Command {
        name: "IsRaining",
        opcode: 0x103E,
        args: &[],
    },
    Command {
        name: "GetAttacked",
        opcode: 0x103F,
        args: &[],
    },
    Command {
        name: "GetIsCreature",
        opcode: 0x1040,
        args: &[],
    },
    Command {
        name: "GetLockLevel",
        opcode: 0x1041,
        args: &[],
    },
    Command {
        name: "GetShouldAttack",
        opcode: 0x1042,
        args: &["Actor"],
    },
    Command {
        name: "GetInCell",
        opcode: 0x1043,
        args: &["Cell"],
    },
    Command {
        name: "GetIsClass",
        opcode: 0x1044,
        args: &["Class"],
    },
    Command {
        name: "GetIsRace",
        opcode: 0x1045,
        args: &["Race"],
    },
    Command {
        name: "GetIsSex",
        opcode: 0x1046,
        args: &["Sex"],
    },
    Command {
        name: "GetInFaction",
        opcode: 0x1047,
        args: &["Faction"],
    },
    Command {
        name: "GetIsID",
        opcode: 0x1048,
        args: &["ObjectID"],
    },
    Command {
        name: "GetFactionRank",
        opcode: 0x1049,
        args: &["Faction"],
    },
    Command {
        name: "GetGlobalValue",
        opcode: 0x104A,
        args: &["Global"],
    },
    Command {
        name: "IsSnowing",
        opcode: 0x104B,
        args: &[],
    },
    Command {
        name: "GetDisposition",
        opcode: 0x104C,
        args: &["Actor"],
    },
    Command {
        name: "GetRandomPercent",
        opcode: 0x104D,
        args: &[],
    },
    Command {
        name: "StreamMusic",
        opcode: 0x104E,
        args: &["String"],
    },
    Command {
        name: "GetQuestVariable",
        opcode: 0x104F,
        args: &["Quest", "Variable Name"],
    },
    Command {
        name: "GetLevel",
        opcode: 0x1050,
        args: &[],
    },
    Command {
        name: "GetArmorRating",
        opcode: 0x1051,
        args: &[],
    },
    Command {
        name: "ModDisposition",
        opcode: 0x1053,
        args: &["Actor", "Value"],
    },
    Command {
        name: "GetDeadCount",
        opcode: 0x1054,
        args: &["Actor Base"],
    },
    Command {
        name: "ShowMap",
        opcode: 0x1055,
        args: &["Map Marker", "Integer"],
    },
    Command {
        name: "Drop",
        opcode: 0x1057,
        args: &["ObjectID", "Count"],
    },
    Command {
        name: "SetAlert",
        opcode: 0x105A,
        args: &["Integer"],
    },
    Command {
        name: "GetIsAlerted",
        opcode: 0x105B,
        args: &[],
    },
    Command {
        name: "StopLook",
        opcode: 0x105D,
        args: &[],
    },
    Command {
        name: "SendAssaultAlarm",
        opcode: 0x105F,
        args: &["Actor", "Faction"],
    },
    Command {
        name: "EnablePlayerControls",
        opcode: 0x1060,
        args: &[
            "Integer", "Integer", "Integer", "Integer", "Integer", "Integer", "Integer",
        ],
    },
    Command {
        name: "DisablePlayerControls",
        opcode: 0x1061,
        args: &[
            "Integer", "Integer", "Integer", "Integer", "Integer", "Integer", "Integer",
        ],
    },
    Command {
        name: "GetPlayerControlsDisabled",
        opcode: 0x1062,
        args: &[
            "Integer", "Integer", "Integer", "Integer", "Integer", "Integer", "Integer",
        ],
    },
    Command {
        name: "GetHeadingAngle",
        opcode: 0x1063,
        args: &["ObjectReferenceID"],
    },
    Command {
        name: "IsWeaponOut",
        opcode: 0x1065,
        args: &[],
    },
    Command {
        name: "IsTorchOut",
        opcode: 0x1066,
        args: &[],
    },
    Command {
        name: "IsShieldOut",
        opcode: 0x1067,
        args: &[],
    },
    Command {
        name: "CreateDetectionEvent",
        opcode: 0x1068,
        args: &["TESObjectREFR", "Integer", "Integer"],
    },
    Command {
        name: "IsActionRef",
        opcode: 0x1069,
        args: &["ObjectReferenceID"],
    },
    Command {
        name: "IsFacingUp",
        opcode: 0x106A,
        args: &[],
    },
    Command {
        name: "GetKnockedState",
        opcode: 0x106B,
        args: &[],
    },
    Command {
        name: "GetWeaponAnimType",
        opcode: 0x106C,
        args: &[],
    },
    Command {
        name: "IsWeaponSkillType",
        opcode: 0x106D,
        args: &["Actor Value"],
    },
    Command {
        name: "GetCurrentAIPackage",
        opcode: 0x106E,
        args: &[],
    },
    Command {
        name: "IsWaiting",
        opcode: 0x106F,
        args: &[],
    },
    Command {
        name: "IsIdlePlaying",
        opcode: 0x1070,
        args: &[],
    },
    Command {
        name: "GetMinorCrimeCount",
        opcode: 0x1074,
        args: &[],
    },
    Command {
        name: "GetMajorCrimeCount",
        opcode: 0x1075,
        args: &[],
    },
    Command {
        name: "GetActorAggroRadiusViolated",
        opcode: 0x1076,
        args: &[],
    },
    Command {
        name: "GetCrimeKnown",
        opcode: 0x1077,
        args: &["Crime Type", "Criminal", "Victim"],
    },
    Command {
        name: "GetCrime",
        opcode: 0x107A,
        args: &["Actor", "Crime Type"],
    },
    Command {
        name: "IsGreetingPlayer",
        opcode: 0x107B,
        args: &[],
    },
    Command {
        name: "StartMisterSandMan",
        opcode: 0x107C,
        args: &["Actor"],
    },
    Command {
        name: "IsGuard",
        opcode: 0x107D,
        args: &[],
    },
    Command {
        name: "StartCannibal",
        opcode: 0x107E,
        args: &["Actor"],
    },
    Command {
        name: "HasBeenEaten",
        opcode: 0x107F,
        args: &[],
    },
    Command {
        name: "GetFatiguePercentage",
        opcode: 0x1080,
        args: &[],
    },
    Command {
        name: "GetPCIsClass",
        opcode: 0x1081,
        args: &["Class"],
    },
    Command {
        name: "GetPCIsRace",
        opcode: 0x1082,
        args: &["Race"],
    },
    Command {
        name: "GetPCIsSex",
        opcode: 0x1083,
        args: &["Sex"],
    },
    Command {
        name: "GetPCInFaction",
        opcode: 0x1084,
        args: &["Faction"],
    },
    Command {
        name: "SameFactionAsPC",
        opcode: 0x1085,
        args: &[],
    },
    Command {
        name: "SameRaceAsPC",
        opcode: 0x1086,
        args: &[],
    },
    Command {
        name: "SameSexAsPC",
        opcode: 0x1087,
        args: &[],
    },
    Command {
        name: "GetIsReference",
        opcode: 0x1088,
        args: &["ObjectReferenceID"],
    },
    Command {
        name: "SetFactionRank",
        opcode: 0x1089,
        args: &["Faction", "Value"],
    },
    Command {
        name: "ModFactionRank",
        opcode: 0x108A,
        args: &["Faction", "Value"],
    },
    Command {
        name: "IsTalking",
        opcode: 0x108D,
        args: &[],
    },
    Command {
        name: "GetWalkSpeed",
        opcode: 0x108E,
        args: &[],
    },
    Command {
        name: "GetCurrentAIProcedure",
        opcode: 0x108F,
        args: &[],
    },
    Command {
        name: "GetTrespassWarningLevel",
        opcode: 0x1090,
        args: &[],
    },
    Command {
        name: "IsTrespassing",
        opcode: 0x1091,
        args: &[],
    },
    Command {
        name: "IsInMyOwnedCell",
        opcode: 0x1092,
        args: &[],
    },
    Command {
        name: "GetWindSpeed",
        opcode: 0x1093,
        args: &[],
    },
    Command {
        name: "GetCurrentWeatherPercent",
        opcode: 0x1094,
        args: &[],
    },
    Command {
        name: "GetIsCurrentWeather",
        opcode: 0x1095,
        args: &["Weather ID"],
    },
    Command {
        name: "IsContinuingPackagePCNear",
        opcode: 0x1096,
        args: &[],
    },
    Command {
        name: "RemoveScriptPackage",
        opcode: 0x1098,
        args: &[],
    },
    Command {
        name: "CanHaveFlames",
        opcode: 0x1099,
        args: &[],
    },
    Command {
        name: "HasFlames",
        opcode: 0x109A,
        args: &[],
    },
    Command {
        name: "AddFlames",
        opcode: 0x109B,
        args: &[],
    },
    Command {
        name: "RemoveFlames",
        opcode: 0x109C,
        args: &[],
    },
    Command {
        name: "GetOpenState",
        opcode: 0x109D,
        args: &[],
    },
    Command {
        name: "GetSitting",
        opcode: 0x109F,
        args: &[],
    },
    Command {
        name: "GetFurnitureMarkerID",
        opcode: 0x10A0,
        args: &[],
    },
    Command {
        name: "GetIsCurrentPackage",
        opcode: 0x10A1,
        args: &["Package"],
    },
    Command {
        name: "IsCurrentFurnitureRef",
        opcode: 0x10A2,
        args: &["ObjectReferenceID"],
    },
    Command {
        name: "IsCurrentFurnitureObj",
        opcode: 0x10A3,
        args: &["Furniture"],
    },
    Command {
        name: "SetSize",
        opcode: 0x10A4,
        args: &["Float"],
    },
    Command {
        name: "GetFactionReaction",
        opcode: 0x10A7,
        args: &["Faction", "Faction"],
    },
    Command {
        name: "SetFactionReaction",
        opcode: 0x10A8,
        args: &["Faction", "Faction", "Value"],
    },
    Command {
        name: "ModFactionReaction",
        opcode: 0x10A9,
        args: &["Faction", "Faction", "Value"],
    },
    Command {
        name: "GetDayOfWeek",
        opcode: 0x10AA,
        args: &[],
    },
    Command {
        name: "GetTalkedToPCParam",
        opcode: 0x10AC,
        args: &["Actor"],
    },
    Command {
        name: "WakeUpPC",
        opcode: 0x10AE,
        args: &["Integer"],
    },
    Command {
        name: "IsPCSleeping",
        opcode: 0x10AF,
        args: &[],
    },
    Command {
        name: "IsPCAMurderer",
        opcode: 0x10B0,
        args: &[],
    },
    Command {
        name: "SelectPlayerSpell",
        opcode: 0x10B3,
        args: &["Spell Item"],
    },
    Command {
        name: "GetDetectionLevel",
        opcode: 0x10B4,
        args: &["Actor"],
    },
    Command {
        name: "IsActorDetected",
        opcode: 0x10B5,
        args: &[],
    },
    Command {
        name: "GetEquipped",
        opcode: 0x10B6,
        args: &["ObjectID"],
    },
    Command {
        name: "Wait",
        opcode: 0x10B7,
        args: &["Package"],
    },
    Command {
        name: "StopWaiting",
        opcode: 0x10B8,
        args: &["Package"],
    },
    Command {
        name: "IsSwimming",
        opcode: 0x10B9,
        args: &[],
    },
    Command {
        name: "ScriptEffectElapsedSeconds",
        opcode: 0x10BA,
        args: &[],
    },
    Command {
        name: "SetCellPublicFlag",
        opcode: 0x10BB,
        args: &["Cell", "Flag"],
    },
    Command {
        name: "GetPCSleepHours",
        opcode: 0x10BC,
        args: &[],
    },
    Command {
        name: "SetPCSleepHours",
        opcode: 0x10BD,
        args: &["Integer"],
    },
    Command {
        name: "GetAmountSoldStolen",
        opcode: 0x10BE,
        args: &[],
    },
    Command {
        name: "ModAmountSoldStolen",
        opcode: 0x10BF,
        args: &["Integer"],
    },
    Command {
        name: "GetIgnoreCrime",
        opcode: 0x10C0,
        args: &[],
    },
    Command {
        name: "GetPCExpelled",
        opcode: 0x10C1,
        args: &["Faction"],
    },
    Command {
        name: "SetPCExpelled",
        opcode: 0x10C2,
        args: &["Faction", "Value"],
    },
    Command {
        name: "GetPCFactionMurder",
        opcode: 0x10C3,
        args: &["Faction"],
    },
    Command {
        name: "SetPCFactionMurder",
        opcode: 0x10C4,
        args: &["Faction", "Value"],
    },
    Command {
        name: "GetPCEnemyofFaction",
        opcode: 0x10C5,
        args: &["Faction"],
    },
    Command {
        name: "SetPCEnemyofFaction",
        opcode: 0x10C6,
        args: &["Faction", "Value"],
    },
    Command {
        name: "GetPCFactionAttack",
        opcode: 0x10C7,
        args: &["Faction"],
    },
    Command {
        name: "SetPCFactionAttack",
        opcode: 0x10C8,
        args: &["Faction", "Value"],
    },
    Command {
        name: "GetDestroyed",
        opcode: 0x10CB,
        args: &[],
    },
    Command {
        name: "GetSelf",
        opcode: 0x10CE,
        args: &[],
    },
    Command {
        name: "GetContainer",
        opcode: 0x10CF,
        args: &[],
    },
    Command {
        name: "GetForceRun",
        opcode: 0x10D0,
        args: &[],
    },
    Command {
        name: "SetForceRun",
        opcode: 0x10D1,
        args: &["Integer"],
    },
    Command {
        name: "GetForceSneak",
        opcode: 0x10D2,
        args: &[],
    },
    Command {
        name: "SetForceSneak",
        opcode: 0x10D3,
        args: &["Integer"],
    },
    Command {
        name: "AdvancePCSkill",
        opcode: 0x10D4,
        args: &["Actor Value", "Amount"],
    },
    Command {
        name: "AdvancePCLevel",
        opcode: 0x10D5,
        args: &[],
    },
    Command {
        name: "HasMagicEffect",
        opcode: 0x10D6,
        args: &["Magic Effect"],
    },
    Command {
        name: "GetDefaultOpen",
        opcode: 0x10D7,
        args: &[],
    },
    Command {
        name: "SetDefaultOpen",
        opcode: 0x10D8,
        args: &["Integer"],
    },
    Command {
        name: "ShowClassMenu",
        opcode: 0x10D9,
        args: &[],
    },
    Command {
        name: "ShowRaceMenu",
        opcode: 0x10DA,
        args: &[],
    },
    Command {
        name: "GetAnimAction",
        opcode: 0x10DB,
        args: &[],
    },
    Command {
        name: "ShowNameMenu",
        opcode: 0x10DC,
        args: &[],
    },
    Command {
        name: "IsSpellTarget",
        opcode: 0x10DF,
        args: &["Magic Item"],
    },
    Command {
        name: "GetVATSMode",
        opcode: 0x10E0,
        args: &[],
    },
    Command {
        name: "GetPersuasionNumber",
        opcode: 0x10E1,
        args: &[],
    },
    Command {
        name: "GetSandman",
        opcode: 0x10E2,
        args: &[],
    },
    Command {
        name: "GetCannibal",
        opcode: 0x10E3,
        args: &[],
    },
    Command {
        name: "GetIsClassDefault",
        opcode: 0x10E4,
        args: &["Class"],
    },
    Command {
        name: "GetClassDefaultMatch",
        opcode: 0x10E5,
        args: &[],
    },
    Command {
        name: "GetInCellParam",
        opcode: 0x10E6,
        args: &["Cell", "ObjectReferenceID"],
    },
    Command {
        name: "SetInChargen",
        opcode: 0x10E7,
        args: &["Integer"],
    },
    Command {
        name: "GetCombatTarget",
        opcode: 0x10E8,
        args: &[],
    },
    Command {
        name: "GetPackageTarget",
        opcode: 0x10E9,
        args: &[],
    },
    Command {
        name: "ShowSpellMaking",
        opcode: 0x10EA,
        args: &[],
    },
    Command {
        name: "GetVatsTargetHeight",
        opcode: 0x10EB,
        args: &[],
    },
    Command {
        name: "GetIsGhost",
        opcode: 0x10ED,
        args: &[],
    },
    Command {
        name: "SetClass",
        opcode: 0x10F0,
        args: &["Class"],
    },
    Command {
        name: "GetUnconscious",
        opcode: 0x10F2,
        args: &[],
    },
    Command {
        name: "GetRestrained",
        opcode: 0x10F4,
        args: &[],
    },
    Command {
        name: "ForceFlee",
        opcode: 0x10F5,
        args: &["Cell", "ObjectReferenceID"],
    },
    Command {
        name: "GetIsUsedItem",
        opcode: 0x10F6,
        args: &["ObjectID"],
    },
    Command {
        name: "GetIsUsedItemType",
        opcode: 0x10F7,
        args: &["Form Type"],
    },
    Command {
        name: "GetIsPlayableRace",
        opcode: 0x10FE,
        args: &[],
    },
    Command {
        name: "GetOffersServicesNow",
        opcode: 0x10FF,
        args: &[],
    },
    Command {
        name: "GetGameSetting",
        opcode: 0x1100,
        args: &["String"],
    },
    Command {
        name: "GetUsedItemLevel",
        opcode: 0x1102,
        args: &[],
    },
    Command {
        name: "GetUsedItemActivate",
        opcode: 0x1103,
        args: &[],
    },
    Command {
        name: "ForceTakeCover",
        opcode: 0x1105,
        args: &["Actor", "Value"],
    },
    Command {
        name: "ModBarterGold",
        opcode: 0x1106,
        args: &["Integer"],
    },
    Command {
        name: "SetBarterGold",
        opcode: 0x1107,
        args: &["Integer"],
    },
    Command {
        name: "GetBarterGold",
        opcode: 0x1108,
        args: &[],
    },
    Command {
        name: "IsTimePassing",
        opcode: 0x1109,
        args: &[],
    },
    Command {
        name: "IsPleasant",
        opcode: 0x110A,
        args: &[],
    },
    Command {
        name: "IsCloudy",
        opcode: 0x110B,
        args: &[],
    },
    Command {
        name: "SetQuestObject",
        opcode: 0x110D,
        args: &["Object", "Integer"],
    },
    Command {
        name: "ForceActorValue",
        opcode: 0x110E,
        args: &["Actor Value", "Amount"],
    },
    Command {
        name: "ModPCSkill",
        opcode: 0x110F,
        args: &["Actor Value", "Amount"],
    },
    Command {
        name: "ModPCAttribute",
        opcode: 0x1110,
        args: &["Actor Value", "Amount"],
    },
    Command {
        name: "GetArmorRatingUpperBody",
        opcode: 0x1112,
        args: &[],
    },
    Command {
        name: "GetParentRef",
        opcode: 0x1113,
        args: &[],
    },
    Command {
        name: "PlayBink",
        opcode: 0x1114,
        args: &["String", "Integer", "Integer", "Integer", "Integer"],
    },
    Command {
        name: "GetBaseActorValue",
        opcode: 0x1115,
        args: &["Actor Value"],
    },
    Command {
        name: "IsOwner",
        opcode: 0x1116,
        args: &["Owner"],
    },
    Command {
        name: "SetOwnership",
        opcode: 0x1117,
        args: &["Owner"],
    },
    Command {
        name: "IsCellOwner",
        opcode: 0x1118,
        args: &["Cell", "Owner"],
    },
    Command {
        name: "SetCellOwnership",
        opcode: 0x1119,
        args: &["Cell", "Owner"],
    },
    Command {
        name: "IsHorseStolen",
        opcode: 0x111A,
        args: &[],
    },
    Command {
        name: "SetCellFullName",
        opcode: 0x111B,
        args: &["Cell", "Message"],
    },
    Command {
        name: "SetActorFullName",
        opcode: 0x111C,
        args: &["Message"],
    },
    Command {
        name: "IsLeftUp",
        opcode: 0x111D,
        args: &[],
    },
    Command {
        name: "IsSneaking",
        opcode: 0x111E,
        args: &[],
    },
    Command {
        name: "IsRunning",
        opcode: 0x111F,
        args: &[],
    },
    Command {
        name: "GetFriendHit",
        opcode: 0x1120,
        args: &[],
    },
    Command {
        name: "IsInCombat",
        opcode: 0x1121,
        args: &[],
    },
    Command {
        name: "SetPackDuration",
        opcode: 0x1122,
        args: &["Float"],
    },
    Command {
        name: "PlayMagicEffectVisuals",
        opcode: 0x1124,
        args: &["Magic Effect", "Time"],
    },
    Command {
        name: "StopMagicEffectVisuals",
        opcode: 0x1126,
        args: &["Magic Effect"],
    },
    Command {
        name: "ResetInterior",
        opcode: 0x1127,
        args: &["Cell"],
    },
    Command {
        name: "IsAnimPlaying",
        opcode: 0x1128,
        args: &["Animation Group"],
    },
    Command {
        name: "SetActorAlpha",
        opcode: 0x1129,
        args: &["Float"],
    },
    Command {
        name: "EnableLinkedPathPoints",
        opcode: 0x112A,
        args: &[],
    },
    Command {
        name: "DisableLinkedPathPoints",
        opcode: 0x112B,
        args: &[],
    },
    Command {
        name: "IsInInterior",
        opcode: 0x112C,
        args: &[],
    },
    Command {
        name: "ForceWeather",
        opcode: 0x112D,
        args: &["Weather ID", "Integer"],
    },
    Command {
        name: "ToggleActorsAI",
        opcode: 0x112E,
        args: &[],
    },
    Command {
        name: "IsActorsAIOff",
        opcode: 0x112F,
        args: &[],
    },
    Command {
        name: "IsWaterObject",
        opcode: 0x1130,
        args: &[],
    },
    Command {
        name: "IsActorUsingATorch",
        opcode: 0x1132,
        args: &[],
    },
    Command {
        name: "SetLevel",
        opcode: 0x1133,
        args: &["Integer", "Integer", "Integer", "Integer"],
    },
    Command {
        name: "ResetFallDamageTimer",
        opcode: 0x1134,
        args: &[],
    },
    Command {
        name: "IsXBox",
        opcode: 0x1135,
        args: &[],
    },
    Command {
        name: "GetInWorldspace",
        opcode: 0x1136,
        args: &["WorldSpace"],
    },
    Command {
        name: "ModPCMiscStat",
        opcode: 0x1137,
        args: &["Miscellaneous Stat", "Integer"],
    },
    Command {
        name: "GetPCMiscStat",
        opcode: 0x1138,
        args: &["Miscellaneous Stat"],
    },
    Command {
        name: "IsActorEvil",
        opcode: 0x1139,
        args: &[],
    },
    Command {
        name: "IsActorAVictim",
        opcode: 0x113A,
        args: &[],
    },
    Command {
        name: "GetTotalPersuasionNumber",
        opcode: 0x113B,
        args: &[],
    },
    Command {
        name: "SetScale",
        opcode: 0x113C,
        args: &["Float"],
    },
    Command {
        name: "ModScale",
        opcode: 0x113D,
        args: &["Float"],
    },
    Command {
        name: "GetIdleDoneOnce",
        opcode: 0x113E,
        args: &[],
    },
    Command {
        name: "KillAllActors",
        opcode: 0x113F,
        args: &["Actor"],
    },
    Command {
        name: "GetNoRumors",
        opcode: 0x1140,
        args: &[],
    },
    Command {
        name: "SetNoRumors",
        opcode: 0x1141,
        args: &["Integer"],
    },
    Command {
        name: "WhichServiceMenu",
        opcode: 0x1143,
        args: &[],
    },
    Command {
        name: "TriggerHitShader",
        opcode: 0x1144,
        args: &["Float"],
    },
    Command {
        name: "FunctionUnused16",
        opcode: 0x1145,
        args: &[],
    },
    Command {
        name: "Reset3DState",
        opcode: 0x1146,
        args: &[],
    },
    Command {
        name: "IsRidingHorse",
        opcode: 0x1147,
        args: &[],
    },
    Command {
        name: "DispelAllSpells",
        opcode: 0x1148,
        args: &[],
    },
    Command {
        name: "DuplicateAllItems",
        opcode: 0x114B,
        args: &["Container", "Ownership Added"],
    },
    Command {
        name: "IsInDangerousWater",
        opcode: 0x114C,
        args: &[],
    },
    Command {
        name: "EssentialDeathReload",
        opcode: 0x114D,
        args: &["String"],
    },
    Command {
        name: "SetShowQuestItems",
        opcode: 0x114E,
        args: &["Integer"],
    },
    Command {
        name: "DuplicateNPCStats",
        opcode: 0x114F,
        args: &["Actor"],
    },
    Command {
        name: "SetIgnoreFriendlyHits",
        opcode: 0x1151,
        args: &["Integer"],
    },
    Command {
        name: "GetIgnoreFriendlyHits",
        opcode: 0x1152,
        args: &[],
    },
    Command {
        name: "IsPlayersLastRiddenHorse",
        opcode: 0x1153,
        args: &[],
    },
    Command {
        name: "SetActorRefraction",
        opcode: 0x1154,
        args: &["Float"],
    },
    Command {
        name: "SetItemValue",
        opcode: 0x1155,
        args: &["Integer"],
    },
    Command {
        name: "SetRigidBodyMass",
        opcode: 0x1156,
        args: &["Float"],
    },
    Command {
        name: "ShowViewerStrings",
        opcode: 0x1157,
        args: &[],
    },
    Command {
        name: "SetAllReachable",
        opcode: 0x1159,
        args: &["Integer"],
    },
    Command {
        name: "SetAllVisible",
        opcode: 0x115A,
        args: &["Integer"],
    },
    Command {
        name: "SetNoAvoidance",
        opcode: 0x115B,
        args: &["Integer"],
    },
    Command {
        name: "SendTrespassAlarm",
        opcode: 0x115C,
        args: &["Actor"],
    },
    Command {
        name: "SetSceneIsComplex",
        opcode: 0x115D,
        args: &["Integer"],
    },
    Command {
        name: "Autosave",
        opcode: 0x115E,
        args: &[],
    },
    Command {
        name: "StartMasterFileSeekData",
        opcode: 0x115F,
        args: &[],
    },
    Command {
        name: "DumpMasterFileSeekData",
        opcode: 0x1160,
        args: &[],
    },
    Command {
        name: "IsActor",
        opcode: 0x1161,
        args: &[],
    },
    Command {
        name: "IsEssential",
        opcode: 0x1162,
        args: &[],
    },
    Command {
        name: "PreloadMagicEffect",
        opcode: 0x1163,
        args: &["Magic Effect"],
    },
    Command {
        name: "ShowDialogSubtitles",
        opcode: 0x1164,
        args: &["Integer"],
    },
    Command {
        name: "IsPlayerMovingIntoNewSpace",
        opcode: 0x1166,
        args: &[],
    },
    Command {
        name: "GetTimeDead",
        opcode: 0x1169,
        args: &[],
    },
    Command {
        name: "GetPlayerHasLastRiddenHorse",
        opcode: 0x116A,
        args: &[],
    },
    Command {
        name: "GetLinkedRef",
        opcode: 0x116B,
        args: &[],
    },
    Command {
        name: "DamageObject",
        opcode: 0x116C,
        args: &["Float"],
    },
    Command {
        name: "IsChild",
        opcode: 0x116D,
        args: &[],
    },
    Command {
        name: "GetLastPlayerAction",
        opcode: 0x116F,
        args: &[],
    },
    Command {
        name: "IsPlayerActionActive",
        opcode: 0x1170,
        args: &["Integer"],
    },
    Command {
        name: "SetTalkingActivatorActor",
        opcode: 0x1171,
        args: &["Actor"],
    },
    Command {
        name: "IsTalkingActivatorActor",
        opcode: 0x1172,
        args: &["Actor"],
    },
    Command {
        name: "ShowBarterMenu",
        opcode: 0x1173,
        args: &["Integer"],
    },
    Command {
        name: "IsInList",
        opcode: 0x1174,
        args: &["Form List"],
    },
    Command {
        name: "ShowHackingMiniGame",
        opcode: 0x1178,
        args: &[],
    },
    Command {
        name: "ShowSurgeryMenu",
        opcode: 0x1179,
        args: &[],
    },
    Command {
        name: "ShowRepairMenu",
        opcode: 0x117A,
        args: &[],
    },
    Command {
        name: "FunctionUnused19",
        opcode: 0x117B,
        args: &[],
    },
    Command {
        name: "AddNote",
        opcode: 0x117C,
        args: &["Note"],
    },
    Command {
        name: "RemoveNote",
        opcode: 0x117D,
        args: &["Note"],
    },
    Command {
        name: "GetHasNote",
        opcode: 0x117E,
        args: &["Note"],
    },
    Command {
        name: "AddToFaction",
        opcode: 0x117F,
        args: &["Faction", "Value"],
    },
    Command {
        name: "RemoveFromFaction",
        opcode: 0x1180,
        args: &["Faction"],
    },
    Command {
        name: "TriggerHUDShudder",
        opcode: 0x1183,
        args: &["Float", "Float", "Float", "Float"],
    },
    Command {
        name: "SetDisposition",
        opcode: 0x1184,
        args: &["Actor", "Value"],
    },
    Command {
        name: "ShowComputersInterface",
        opcode: 0x1185,
        args: &[],
    },
    Command {
        name: "SetGlobalTimeMultiplier",
        opcode: 0x1186,
        args: &["Float", "Integer"],
    },
    Command {
        name: "GetHitLocation",
        opcode: 0x1187,
        args: &[],
    },
    Command {
        name: "IsPC1stPerson",
        opcode: 0x1188,
        args: &[],
    },
    Command {
        name: "PushActorAway",
        opcode: 0x118A,
        args: &["ObjectReferenceID", "Integer"],
    },
    Command {
        name: "SetActorsAI",
        opcode: 0x118B,
        args: &["Integer"],
    },
    Command {
        name: "ClearOwnership",
        opcode: 0x118C,
        args: &[],
    },
    Command {
        name: "GetCauseofDeath",
        opcode: 0x118D,
        args: &[],
    },
    Command {
        name: "IsLimbGone",
        opcode: 0x118E,
        args: &["Integer"],
    },
    Command {
        name: "IsWeaponInList",
        opcode: 0x118F,
        args: &["Form List"],
    },
    Command {
        name: "PlayIdle",
        opcode: 0x1190,
        args: &["String"],
    },
    Command {
        name: "RemoveImageSpaceModifier",
        opcode: 0x1192,
        args: &["Imagespace Modifier ID", "Float"],
    },
    Command {
        name: "HasFriendDisposition",
        opcode: 0x1193,
        args: &[],
    },
    Command {
        name: "FunctionUnused20",
        opcode: 0x1194,
        args: &[],
    },
    Command {
        name: "FriendDispositionBoost",
        opcode: 0x1195,
        args: &[],
    },
    Command {
        name: "SetCellImageSpace",
        opcode: 0x1196,
        args: &["Cell", "ImageSpace"],
    },
    Command {
        name: "ShowChargenMenu",
        opcode: 0x1197,
        args: &[],
    },
    Command {
        name: "GetVATSValue",
        opcode: 0x1198,
        args: &["Integer", "Integer"],
    },
    Command {
        name: "IsKiller",
        opcode: 0x1199,
        args: &["Actor"],
    },
    Command {
        name: "IsKillerObject",
        opcode: 0x119A,
        args: &["Form List"],
    },
    Command {
        name: "GetFactionCombatReaction",
        opcode: 0x119B,
        args: &["Faction", "Faction"],
    },
    Command {
        name: "UseWeapon",
        opcode: 0x119C,
        args: &[
            "Weapon",
            "Location",
            "Target",
            "NumAttacks",
            "CrouchToReload",
            "HoldFire",
            "AlwaysHit",
            "DoNoDamage",
            "TargetLocation",
        ],
    },
    Command {
        name: "EvaluateSpellConditions",
        opcode: 0x119D,
        args: &[],
    },
    Command {
        name: "ToggleMotionBlur",
        opcode: 0x119E,
        args: &[],
    },
    Command {
        name: "Exists",
        opcode: 0x119F,
        args: &["ObjectReferenceID"],
    },
    Command {
        name: "GetGroupMemberCount",
        opcode: 0x11A0,
        args: &[],
    },
    Command {
        name: "GetGroupTargetCount",
        opcode: 0x11A1,
        args: &[],
    },
    Command {
        name: "SetObjectiveCompleted",
        opcode: 0x11A2,
        args: &["Quest", "ObjectiveIndex", "State"],
    },
    Command {
        name: "SetObjectiveDisplayed",
        opcode: 0x11A3,
        args: &["Quest", "ObjectiveIndex", "State"],
    },
    Command {
        name: "GetObjectiveCompleted",
        opcode: 0x11A4,
        args: &["Quest", "ObjectiveIndex"],
    },
    Command {
        name: "GetObjectiveDisplayed",
        opcode: 0x11A5,
        args: &["Quest", "ObjectiveIndex"],
    },
    Command {
        name: "SetImageSpace",
        opcode: 0x11A6,
        args: &["ImageSpace"],
    },
    Command {
        name: "PipboyRadio",
        opcode: 0x11A7,
        args: &["String", "ObjectReferenceID"],
    },
    Command {
        name: "RemovePerk",
        opcode: 0x11A8,
        args: &["Perk", "Alt"],
    },
    Command {
        name: "DisableAllActors",
        opcode: 0x11A9,
        args: &[],
    },
    Command {
        name: "GetIsFormType",
        opcode: 0x11AA,
        args: &["String"],
    },
    Command {
        name: "GetIsVoiceType",
        opcode: 0x11AB,
        args: &["VoiceType"],
    },
    Command {
        name: "GetPlantedExplosive",
        opcode: 0x11AC,
        args: &[],
    },
    Command {
        name: "CompleteAllObjectives",
        opcode: 0x11AD,
        args: &["Quest"],
    },
    Command {
        name: "IsActorTalkingThroughActivator",
        opcode: 0x11AE,
        args: &[],
    },
    Command {
        name: "GetHealthPercentage",
        opcode: 0x11AF,
        args: &[],
    },
    Command {
        name: "SetAudioMultithreading",
        opcode: 0x11B0,
        args: &["Integer"],
    },
    Command {
        name: "GetIsObjectType",
        opcode: 0x11B1,
        args: &["Form Type"],
    },
    Command {
        name: "ShowChargenMenuParams",
        opcode: 0x11B2,
        args: &["Integer", "Integer", "Integer"],
    },
    Command {
        name: "GetDialogueEmotion",
        opcode: 0x11B3,
        args: &[],
    },
    Command {
        name: "GetDialogueEmotionValue",
        opcode: 0x11B4,
        args: &[],
    },
    Command {
        name: "GetIsCreatureType",
        opcode: 0x11B6,
        args: &["Integer"],
    },
    Command {
        name: "SetMerchantContainer",
        opcode: 0x11B7,
        args: &["ObjectReferenceID"],
    },
    Command {
        name: "RemoveMerchantContainer",
        opcode: 0x11B8,
        args: &[],
    },
    Command {
        name: "ShowWarning",
        opcode: 0x11B9,
        args: &["String"],
    },
    Command {
        name: "EnterTrigger",
        opcode: 0x11BA,
        args: &["ObjectReferenceID"],
    },
    Command {
        name: "AddItemHealthPercent",
        opcode: 0x11BC,
        args: &["ObjectID", "Count", "Float", "Flag"],
    },
    Command {
        name: "PlaceAtMeHealthPercent",
        opcode: 0x11BD,
        args: &[
            "ObjectID",
            "Health Percent",
            "Count",
            "Distance",
            "Direction",
        ],
    },
    Command {
        name: "GetInZone",
        opcode: 0x11BE,
        args: &["EncounterZone"],
    },
    Command {
        name: "DisableNavMesh",
        opcode: 0x11BF,
        args: &["String"],
    },
    Command {
        name: "HasPerk",
        opcode: 0x11C1,
        args: &["Perk", "Alt"],
    },
    Command {
        name: "GetFactionRelation",
        opcode: 0x11C2,
        args: &["Actor"],
    },
    Command {
        name: "IsLastIdlePlayed",
        opcode: 0x11C3,
        args: &["Idle Form"],
    },
    Command {
        name: "SetNPCRadio",
        opcode: 0x11C4,
        args: &["Integer", "ObjectReferenceID"],
    },
    Command {
        name: "SetPlayerTeammate",
        opcode: 0x11C5,
        args: &["Integer"],
    },
    Command {
        name: "GetPlayerTeammate",
        opcode: 0x11C6,
        args: &[],
    },
    Command {
        name: "GetPlayerTeammateCount",
        opcode: 0x11C7,
        args: &[],
    },
    Command {
        name: "ClearActorsFactionsPlayerEnemyFlag",
        opcode: 0x11CA,
        args: &[],
    },
    Command {
        name: "GetActorCrimePlayerEnemy",
        opcode: 0x11CB,
        args: &[],
    },
    Command {
        name: "GetActorFactionPlayerEnemy",
        opcode: 0x11CC,
        args: &[],
    },
    Command {
        name: "IsPlayerTagSkill",
        opcode: 0x11CE,
        args: &["Actor Value"],
    },
    Command {
        name: "GetPlayerGrabbedRef",
        opcode: 0x11CF,
        args: &[],
    },
    Command {
        name: "IsPlayerGrabbedRef",
        opcode: 0x11D0,
        args: &["TESObjectREFR"],
    },
    Command {
        name: "PlaceLeveledActorAtMe",
        opcode: 0x11D1,
        args: &["ObjectID", "LevelModifier", "ForcedZone"],
    },
    Command {
        name: "ShowLockpickMenu",
        opcode: 0x11D3,
        args: &["Integer"],
    },
    Command {
        name: "GetBroadcastState",
        opcode: 0x11D4,
        args: &[],
    },
    Command {
        name: "SetBroadcastState",
        opcode: 0x11D5,
        args: &["Integer"],
    },
    Command {
        name: "StartRadioConversation",
        opcode: 0x11D6,
        args: &["Topic"],
    },
    Command {
        name: "GetDestructionStage",
        opcode: 0x11D7,
        args: &[],
    },
    Command {
        name: "ClearDestruction",
        opcode: 0x11D8,
        args: &[],
    },
    Command {
        name: "CastImmediateOnSelf",
        opcode: 0x11D9,
        args: &["Magic Item"],
    },
    Command {
        name: "GetIsAlignment",
        opcode: 0x11DA,
        args: &["Alignment"],
    },
    Command {
        name: "ResetQuest",
        opcode: 0x11DB,
        args: &["Quest"],
    },
    Command {
        name: "SetQuestDelay",
        opcode: 0x11DC,
        args: &["Quest", "Float"],
    },
    Command {
        name: "ForceActiveQuest",
        opcode: 0x11DD,
        args: &["Quest"],
    },
    Command {
        name: "GetThreatRatio",
        opcode: 0x11DE,
        args: &["Actor"],
    },
    Command {
        name: "MatchFaceGeometry",
        opcode: 0x11DF,
        args: &["Actor", "Value"],
    },
    Command {
        name: "GetIsUsedItemEquipType",
        opcode: 0x11E0,
        args: &["EquipType"],
    },
    Command {
        name: "GetPlayerName",
        opcode: 0x11E1,
        args: &[],
    },
    Command {
        name: "FireWeapon",
        opcode: 0x11E2,
        args: &["Weapon"],
    },
    Command {
        name: "ShowTutorialMenu",
        opcode: 0x11E3,
        args: &["Message"],
    },
    Command {
        name: "AgeRace",
        opcode: 0x11E4,
        args: &["Integer"],
    },
    Command {
        name: "MatchRace",
        opcode: 0x11E5,
        args: &["Actor"],
    },
    Command {
        name: "SetPCYoung",
        opcode: 0x11E6,
        args: &["Integer"],
    },
    Command {
        name: "SexChange",
        opcode: 0x11E7,
        args: &["Sex", "Integer"],
    },
    Command {
        name: "ShowSPECIALBookMenu",
        opcode: 0x11E8,
        args: &[],
    },
    Command {
        name: "GetConcussed",
        opcode: 0x11E9,
        args: &[],
    },
    Command {
        name: "SetZoneRespawns",
        opcode: 0x11EA,
        args: &["EncounterZone", "Integer"],
    },
    Command {
        name: "SetVATSTarget",
        opcode: 0x11EB,
        args: &["Integer"],
    },
    Command {
        name: "GetMapMarkerVisible",
        opcode: 0x11EC,
        args: &[],
    },
    Command {
        name: "ResetInventory",
        opcode: 0x11ED,
        args: &[],
    },
    Command {
        name: "ShowSPECIALBookMenuParams",
        opcode: 0x11EE,
        args: &["Integer"],
    },
    Command {
        name: "GetPermanentActorValue",
        opcode: 0x11EF,
        args: &["Actor Value"],
    },
    Command {
        name: "GetKillingBlowLimb",
        opcode: 0x11F0,
        args: &[],
    },
    Command {
        name: "ShowBarberMenu",
        opcode: 0x11F1,
        args: &[],
    },
    Command {
        name: "ShowPlasticSurgeonMenu",
        opcode: 0x11F2,
        args: &[],
    },
    Command {
        name: "TriggerLODApocalypse",
        opcode: 0x11F3,
        args: &[],
    },
    Command {
        name: "GetWeaponHealthPerc",
        opcode: 0x11F4,
        args: &[],
    },
    Command {
        name: "SetWeaponHealthPerc",
        opcode: 0x11F5,
        args: &["Float"],
    },
    Command {
        name: "ModWeaponHealthPerc",
        opcode: 0x11F6,
        args: &["Float"],
    },
    Command {
        name: "GetRadiationLevel",
        opcode: 0x11F7,
        args: &[],
    },
    Command {
        name: "ShowAllMapMarkers",
        opcode: 0x11F8,
        args: &["Integer", "Integer", "Integer"],
    },
    Command {
        name: "ShowChargenMenuModValues",
        opcode: 0x11F9,
        args: &["Integer", "Integer", "Integer"],
    },
    Command {
        name: "ResetAI",
        opcode: 0x11FA,
        args: &[],
    },
    Command {
        name: "SetRumble",
        opcode: 0x11FB,
        args: &["Float", "Float", "Float"],
    },
    Command {
        name: "SetNoActivationSound",
        opcode: 0x11FC,
        args: &["Sound"],
    },
    Command {
        name: "ClearNoActivationSound",
        opcode: 0x11FD,
        args: &[],
    },
    Command {
        name: "GetLastHitCritical",
        opcode: 0x11FE,
        args: &[],
    },
    Command {
        name: "PlayMusic",
        opcode: 0x11FF,
        args: &["Music"],
    },
    Command {
        name: "SetLocationSpecificLoadScreensOnly",
        opcode: 0x1200,
        args: &["Integer"],
    },
    Command {
        name: "ResetPipboyManager",
        opcode: 0x1201,
        args: &[],
    },
    Command {
        name: "SetPCToddler",
        opcode: 0x1202,
        args: &["Integer"],
    },
    Command {
        name: "IsCombatTarget",
        opcode: 0x1203,
        args: &["Actor"],
    },
    Command {
        name: "RewardKarma",
        opcode: 0x1204,
        args: &["Integer"],
    },
    Command {
        name: "TriggerScreenBlood",
        opcode: 0x1205,
        args: &["Integer"],
    },
    Command {
        name: "GetVATSRightAreaFree",
        opcode: 0x1206,
        args: &["TESObjectREFR"],
    },
    Command {
        name: "GetVATSLeftAreaFree",
        opcode: 0x1207,
        args: &["TESObjectREFR"],
    },
    Command {
        name: "GetVATSBackAreaFree",
        opcode: 0x1208,
        args: &["TESObjectREFR"],
    },
    Command {
        name: "GetVATSFrontAreaFree",
        opcode: 0x1209,
        args: &["TESObjectREFR"],
    },
    Command {
        name: "GetIsLockBroken",
        opcode: 0x120A,
        args: &[],
    },
    Command {
        name: "IsPS3",
        opcode: 0x120B,
        args: &[],
    },
    Command {
        name: "IsWin32",
        opcode: 0x120C,
        args: &[],
    },
    Command {
        name: "GetVATSRightTargetVisible",
        opcode: 0x120D,
        args: &["TESObjectREFR"],
    },
    Command {
        name: "GetVATSLeftTargetVisible",
        opcode: 0x120E,
        args: &["TESObjectREFR"],
    },
    Command {
        name: "GetVATSBackTargetVisible",
        opcode: 0x120F,
        args: &["TESObjectREFR"],
    },
    Command {
        name: "GetVATSFrontTargetVisible",
        opcode: 0x1210,
        args: &["TESObjectREFR"],
    },
    Command {
        name: "IsInCriticalStage",
        opcode: 0x1213,
        args: &["CriticalStage"],
    },
    Command {
        name: "RemoveFromAllFactions",
        opcode: 0x1214,
        args: &[],
    },
    Command {
        name: "GetXPForNextLevel",
        opcode: 0x1215,
        args: &[],
    },
    Command {
        name: "ShowLockpickMenuDebug",
        opcode: 0x1216,
        args: &["Integer"],
    },
    Command {
        name: "ForceSave",
        opcode: 0x1217,
        args: &[],
    },
    Command {
        name: "SetSPECIALPoints",
        opcode: 0x1218,
        args: &["Integer"],
    },
    Command {
        name: "AddSPECIALPoints",
        opcode: 0x1219,
        args: &["Integer"],
    },
    Command {
        name: "SetTagSkills",
        opcode: 0x121A,
        args: &["Integer", "Integer"],
    },
    Command {
        name: "AddTagSkills",
        opcode: 0x121B,
        args: &["Integer"],
    },
    Command {
        name: "Sin",
        opcode: 0x121C,
        args: &["Float", "Int"],
    },
    Command {
        name: "Cos",
        opcode: 0x121D,
        args: &["Float", "Int"],
    },
    Command {
        name: "Tan",
        opcode: 0x121E,
        args: &["Float", "Int"],
    },
    Command {
        name: "Sqrt",
        opcode: 0x121F,
        args: &["Float"],
    },
    Command {
        name: "Log",
        opcode: 0x1220,
        args: &["Float", "Float"],
    },
    Command {
        name: "Abs",
        opcode: 0x1221,
        args: &["Float"],
    },
    Command {
        name: "GetQuestCompleted",
        opcode: 0x1222,
        args: &["Quest"],
    },
    Command {
        name: "ForceTerminalBack",
        opcode: 0x1223,
        args: &[],
    },
    Command {
        name: "PipBoyRadioOff",
        opcode: 0x1224,
        args: &[],
    },
    Command {
        name: "AutoDisplayObjectives",
        opcode: 0x1225,
        args: &["Integer"],
    },
    Command {
        name: "IsGoreDisabled",
        opcode: 0x1226,
        args: &[],
    },
    Command {
        name: "FadeSFX",
        opcode: 0x1227,
        args: &["Float"],
    },
    Command {
        name: "SetMinimalUse",
        opcode: 0x1228,
        args: &["Integer"],
    },
    Command {
        name: "SetPCCanUsePowerArmor",
        opcode: 0x1229,
        args: &["Integer"],
    },
    Command {
        name: "ShowQuestStages",
        opcode: 0x122A,
        args: &["Quest"],
    },
    Command {
        name: "GetSpellUsageNum",
        opcode: 0x122B,
        args: &["Magic Item"],
    },
    Command {
        name: "ForceRadioStationUpdate",
        opcode: 0x122C,
        args: &[],
    },
    Command {
        name: "GetActorsInHigh",
        opcode: 0x122D,
        args: &[],
    },
    Command {
        name: "HasLoaded3D",
        opcode: 0x122E,
        args: &[],
    },
    Command {
        name: "DisableAllMines",
        opcode: 0x122F,
        args: &[],
    },
    Command {
        name: "SetLastExtDoorActivated",
        opcode: 0x1230,
        args: &["ObjectReferenceID"],
    },
    Command {
        name: "KillQuestUpdates",
        opcode: 0x1231,
        args: &[],
    },
    Command {
        name: "IsImageSpaceActive",
        opcode: 0x1232,
        args: &["Imagespace Modifier ID"],
    },
    Command {
        name: "AddFormToFormList",
        opcode: 0x1233,
        args: &["Form List", "Any Form"],
    },
    Command {
        name: "AddNPCToLeveledList",
        opcode: 0x1234,
        args: &["Leveled Character", "NPC or Leveled Character", "Level"],
    },
    Command {
        name: "AddCreatureToLeveledList",
        opcode: 0x1235,
        args: &["Leveled Creature", "Creature or Leveled Creature", "Level"],
    },
    Command {
        name: "AddItemToLeveledList",
        opcode: 0x1236,
        args: &[
            "Leveled Item",
            "Object ID",
            "Level",
            "Count",
            "Health Percent",
        ],
    },
    Command {
        name: "RemapWaterType",
        opcode: 0x1237,
        args: &["Form", "Form"],
    },
    Command {
        name: "SetReputation",
        opcode: 0x1238,
        args: &["Reputation", "Int", "Float"],
    },
    Command {
        name: "RemoveReputation",
        opcode: 0x123A,
        args: &["Reputation", "Int", "Int"],
    },
    Command {
        name: "AddReputationExact",
        opcode: 0x123B,
        args: &["Reputation", "Int", "Float"],
    },
    Command {
        name: "RemoveReputationExact",
        opcode: 0x123C,
        args: &["Reputation", "Int", "Float"],
    },
    Command {
        name: "GetReputation",
        opcode: 0x123D,
        args: &["Reputation", "Int"],
    },
    Command {
        name: "GetReputationPct",
        opcode: 0x123E,
        args: &["Reputation", "Int"],
    },
    Command {
        name: "GetReputationThreshold",
        opcode: 0x123F,
        args: &["Reputation", "Int"],
    },
    Command {
        name: "TriggerScreenSplatter",
        opcode: 0x1240,
        args: &["Integer", "Float", "Float", "Float"],
    },
    Command {
        name: "ClearScreenSplatter",
        opcode: 0x1241,
        args: &[],
    },
    Command {
        name: "SetScreenSplatterTexture",
        opcode: 0x1242,
        args: &["String", "Int"],
    },
    Command {
        name: "SetScreenSplatterFade",
        opcode: 0x1243,
        args: &["Float", "Int"],
    },
    Command {
        name: "ShowLoveTesterMenu",
        opcode: 0x1244,
        args: &[],
    },
    Command {
        name: "ShowLoveTesterMenuParams",
        opcode: 0x1245,
        args: &["Integer"],
    },
    Command {
        name: "SetCameraFOV",
        opcode: 0x1246,
        args: &["Float", "Float"],
    },
    Command {
        name: "ShowTraitSelectMenu",
        opcode: 0x1247,
        args: &[],
    },
    Command {
        name: "SetSecuritronExpression",
        opcode: 0x1248,
        args: &["ObjectReferenceID", "String", "String"],
    },
    Command {
        name: "RemoveAllTypedItems",
        opcode: 0x1249,
        args: &[
            "Container",
            "Ownership Added",
            "No Message Flag",
            "Type",
            "Holdout List",
        ],
    },
    Command {
        name: "IsHardcore",
        opcode: 0x124A,
        args: &[],
    },
    Command {
        name: "SetHardcore",
        opcode: 0x124B,
        args: &["Integer"],
    },
    Command {
        name: "ToggleDemigodMode",
        opcode: 0x124C,
        args: &[],
    },
    Command {
        name: "DetonatePlacedExplosives",
        opcode: 0x124D,
        args: &[],
    },
    Command {
        name: "GetOwnerLastTarget",
        opcode: 0x124E,
        args: &[],
    },
    Command {
        name: "MoveToMarkerWithFade",
        opcode: 0x124F,
        args: &["ObjectReferenceID", "Float", "Float", "Float"],
    },
    Command {
        name: "ShowRecipeMenu",
        opcode: 0x1250,
        args: &["Form"],
    },
    Command {
        name: "ShowSlotMachineMenu",
        opcode: 0x1251,
        args: &[],
    },
    Command {
        name: "ShowSlotMachineMenuParams",
        opcode: 0x1252,
        args: &["Casino", "Min Bet", "Max Bet", "Max Winnings"],
    },
    Command {
        name: "ShowBlackJackMenu",
        opcode: 0x1253,
        args: &[],
    },
    Command {
        name: "ShowBlackJackMenuParams",
        opcode: 0x1254,
        args: &["Casino", "Min Bet", "Max Bet", "Max Winnings"],
    },
    Command {
        name: "ShowRouletteMenu",
        opcode: 0x1255,
        args: &[],
    },
    Command {
        name: "ShowRouletteMenuParams",
        opcode: 0x1256,
        args: &["Casino", "Min Bet", "Max Bet", "Max Winnings"],
    },
    Command {
        name: "IsCodeRedeemed",
        opcode: 0x1257,
        args: &["String"],
    },
    Command {
        name: "RedeemCode",
        opcode: 0x1258,
        args: &["String"],
    },
    Command {
        name: "GetForceHitReaction",
        opcode: 0x1259,
        args: &[],
    },
    Command {
        name: "ShowCaravanMenu",
        opcode: 0x125A,
        args: &["Caravan Deck", "AI TYPE", "BET PERCENTAGE"],
    },
    Command {
        name: "AddCardToPlayer",
        opcode: 0x125B,
        args: &[],
    },
    Command {
        name: "GetCasinoWinningsLevel",
        opcode: 0x125C,
        args: &["Casino"],
    },
    Command {
        name: "SetCasinoWinningsLevel",
        opcode: 0x125D,
        args: &["Casino", "Winning Level"],
    },
    Command {
        name: "UnlockChallenge",
        opcode: 0x125E,
        args: &["Challenge"],
    },
    Command {
        name: "ChallengeLocked",
        opcode: 0x125F,
        args: &["Challenge"],
    },
    Command {
        name: "IsPlayerInRegion",
        opcode: 0x1260,
        args: &["Region"],
    },
    Command {
        name: "SetCasinoCheatLevel",
        opcode: 0x1261,
        args: &["Integer"],
    },
    Command {
        name: "GetCasinoWinningStage",
        opcode: 0x1262,
        args: &["Casino"],
    },
    Command {
        name: "PlaceAtReticle",
        opcode: 0x1263,
        args: &["ObjectID", "Count", "Float", "Float"],
    },
    Command {
        name: "PlayerInRegion",
        opcode: 0x1264,
        args: &["Region"],
    },
    Command {
        name: "ObjectUnderReticle",
        opcode: 0x1265,
        args: &["MinDistance", "MaxDistance"],
    },
    Command {
        name: "GetChallengeCompleted",
        opcode: 0x1266,
        args: &["Challenge"],
    },
    Command {
        name: "RevokeCode",
        opcode: 0x1267,
        args: &["String"],
    },
    Command {
        name: "ShowTraitMenu",
        opcode: 0x1268,
        args: &[],
    },
    Command {
        name: "SwapTextureOnRef",
        opcode: 0x1269,
        args: &["ObjectReferenceID", "String", "String"],
    },
    Command {
        name: "DisableHardcoreTracking",
        opcode: 0x126A,
        args: &[],
    },
    Command {
        name: "IsAlwaysHardcore",
        opcode: 0x126B,
        args: &[],
    },
    Command {
        name: "RemoveRecurringFromChallenge",
        opcode: 0x126C,
        args: &["Challenge"],
    },
    Command {
        name: "ResetXP",
        opcode: 0x126D,
        args: &[],
    },
    Command {
        name: "GetDLCCount",
        opcode: 0x126E,
        args: &[],
    },
    Command {
        name: "IsDLCInstalled",
        opcode: 0x126F,
        args: &["String"],
    },
    Command {
        name: "EnableLoadingMenu",
        opcode: 0x1270,
        args: &["Integer"],
    },
    Command {
        name: "IsLoadingMenuEnabled",
        opcode: 0x1271,
        args: &[],
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
const ALIASES: &[(&str, &str)] = &[
    ("GetAV", "GetActorValue"),
    ("SetActorValue", "SetAV"),
    ("ModAV", "ModActorValue"),
    ("GetLOS", "GetLineOfSight"),
    ("GetQR", "GetQuestRunning"),
    ("EvaluatePackage", "EVP"),
    ("GetFatigue", "GetFatiguePercentage"),
    ("kill", "KillActor"),
    ("ResurrectActor", "Resurrect"),
    ("GetWalk", "GetWalkSpeed"),
    ("getweatherpct", "GetCurrentWeatherPercent"),
    ("getweather", "GetIsCurrentWeather"),
    ("MoveToMarker", "MoveTo"),
    ("CSize", "SetSize"),
    ("setcs", "SetCombatStyle"),
    ("spspell", "SelectPlayerSpell"),
    ("setpublic", "SetCellPublicFlag"),
    ("getAR", "GetActionRef"),
    ("this", "GetSelf"),
    ("AdvSkill", "AdvancePCSkill"),
    ("AdvLevel", "AdvancePCLevel"),
    ("EquipObject", "EquipItem"),
    ("UnEquipObject", "UnequipItem"),
    ("Flee", "ForceFlee"),
    ("GetGS", "GetGameSetting"),
    ("StopCombatAlarmOnActor", "SCAOnActor"),
    ("sw", "SetWeather"),
    ("TakeCover", "ForceTakeCover"),
    ("ForceAV", "ForceActorValue"),
    ("ModPCS", "ModPCSkill"),
    ("ModPCA", "ModPCAttribute"),
    ("EnableFast", "EnableFastTravel"),
    ("GetBaseAV", "GetBaseActorValue"),
    ("SPDur", "SetPackDuration"),
    ("PlayMagicShaderVisuals", "PMS"),
    ("PME", "PlayMagicEffectVisuals"),
    ("StopMagicShaderVisuals", "SMS"),
    ("SME", "StopMagicEffectVisuals"),
    ("SAA", "SetActorAlpha"),
    ("fw", "ForceWeather"),
    ("ModPCMS", "ModPCMiscStat"),
    ("GetPCMS", "GetPCMiscStat"),
    ("killall", "KillAllActors"),
    ("ths", "TriggerHitShader"),
    ("sifh", "SetIgnoreFriendlyHits"),
    ("gifh", "GetIgnoreFriendlyHits"),
    ("sar", "SetActorRefraction"),
    ("svs", "ShowViewerStrings"),
    ("rwo", "ReleaseWeatherOverride"),
    ("do", "DamageObject"),
    ("sbm", "ShowBarterMenu"),
    ("shmg", "ShowHackingMiniGame"),
    ("ssmg", "ShowSurgeryMenu"),
    ("srm", "ShowRepairMenu"),
    ("unused", "FunctionUnused19"),
    ("AN", "AddNote"),
    ("RN", "RemoveNote"),
    ("GetN", "GetHasNote"),
    ("Addfac", "AddToFaction"),
    ("Removefac", "RemoveFromFaction"),
    ("DamageActorValue", "DamageAV"),
    ("RestoreActorValue", "RestoreAV"),
    ("hudsh", "TriggerHUDShudder"),
    ("SetDisp", "SetDisposition"),
    ("sci", "ShowComputersInterface"),
    ("sgtm", "SetGlobalTimeMultiplier"),
    ("pc1st", "IsPC1stPerson"),
    ("pcb", "PurgeCellBuffers"),
    ("imod", "ApplyImageSpaceModifier"),
    ("rimod", "RemoveImageSpaceModifier"),
    ("scgm", "ShowChargenMenu"),
    ("esc", "EvaluateSpellConditions"),
    ("tmb", "ToggleMotionBlur"),
    ("prad", "PipboyRadio"),
    ("DisAA", "DisableAllActors"),
    ("SAM", "SetAudioMultithreading"),
    ("scgmp", "ShowChargenMenuParams"),
    ("exit", "ExitGame"),
    ("snr", "SetNPCRadio"),
    ("OpenTeammateContainer", "BWaitersServeWine"),
    ("SetPlayerTagSkill", "GetAV"),
    ("slpm", "ShowLockpickMenu"),
    ("cios", "CastImmediateOnSelf"),
    ("ssbm", "ShowSPECIALBookMenu"),
    ("ssbmp", "ShowSPECIALBookMenuParams"),
    ("GetPermAV", "GetPermanentActorValue"),
    ("tmm", "ShowAllMapMarkers"),
    ("scgmod", "ShowChargenMenuModValues"),
    ("tsb", "TriggerScreenBlood"),
    ("slpmd", "ShowLockpickMenuDebug"),
    ("GetQC", "GetQuestCompleted"),
    ("FSFX", "FadeSFX"),
    ("SQS", "ShowQuestStages"),
    ("FRSU", "ForceRadioStationUpdate"),
    ("KQU", "KillQuestUpdates"),
    ("tssp", "TriggerScreenSplatter"),
    ("cssp", "ClearScreenSplatter"),
    ("sspt", "SetScreenSplatterTexture"),
    ("sspf", "SetScreenSplatterFade"),
    ("sltm", "ShowLoveTesterMenu"),
    ("sltmp", "ShowLoveTesterMenuParams"),
    ("FOV", "SetCameraFOV"),
    ("stsm", "ShowTraitSelectMenu"),
    ("setsecexp", "SetSecuritronExpression"),
    ("TDM", "ToggleDemigodMode"),
    ("dpe", "DetonatePlacedExplosives"),
    ("golt", "GetOwnerLastTarget"),
    ("MoveToFade", "MoveToMarkerWithFade"),
    ("craft", "ShowRecipeMenu"),
    ("sslm", "ShowSlotMachineMenu"),
    ("sslmp", "ShowSlotMachineMenuParams"),
    ("sbjm", "ShowBlackJackMenu"),
    ("sbjmp", "ShowBlackJackMenuParams"),
    ("srmp", "ShowRouletteMenuParams"),
    ("crvn", "ShowCaravanMenu"),
    ("addcard", "AddCardToPlayer"),
    ("casinocheat", "SetCasinoCheatLevel"),
    ("traitmenu", "ShowTraitMenu"),
    ("SwapTexture", "SwapTextureOnRef"),
    ("babby", "DisableHardcoreTracking"),
    ("behemoth", "IsAlwaysHardcore"),
    ("rrfc", "RemoveRecurringFromChallenge"),
    ("dlccount", "GetDLCCount"),
    ("hasdlc", "IsDLCInstalled"),
    ("LoadMenuCheck", "IsLoadingMenuEnabled"),
    ("Kill", "KillActor"),
    ("SetActorValue", "SetAV"),
];

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

/// The opcode a command call writes when it appears as an expression value
/// (`0x58`-prefixed), which is not always the opcode its statement call
/// writes.
///
/// Measured: every one of the corpus's 52 expression-form GetAV/
/// GetActorValue calls writes `0x100E`, and its 23 statement-form `GetAV`
/// calls write `0x11CD` — the same source name resolves to a different
/// opcode by position. An expression that resolved `GetAV` to the statement
/// opcode compiles a condition the game reads as a different function.
pub fn expression_opcode(name: &str) -> Option<u16> {
    if name.eq_ignore_ascii_case("GetAV") {
        return Some(0x100E);
    }
    command_called(name).map(|c| c.opcode)
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
