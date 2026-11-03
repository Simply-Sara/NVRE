# obscript bytecode

The format NVRE's compiler writes and reads, derived entirely from the games'
own bytes. Every rule below names the unit that measured it, because the
authoritative spec does not exist: what does is 2,576 `SCPT` records in
`FalloutNV.esm` and 1,257 in `Fallout3.esm`, each carrying both its source
(`SCTX`) and the bytecode the GECK compiled for it (`SCDA`).

The parity harness compiles every source and compares the result with the
game's bytes:

```text
NVRE_DATA=<Data> cargo run --release -p nvre-script --example parity_all
NVRE_DATA=<Data> NVRE_PLUGIN=Fallout3.esm cargo run --release -p nvre-script --example parity_all
```

| corpus | byte-equal |
|---|---|
| `FalloutNV.esm` | 2,576 / 2,576 (100.00%) |
| `Fallout3.esm`  | 1,256 / 1,257 (99.92%) |

`examples/why.rs` explains a unit that differs, instruction by instruction, with
the source statement behind each one; `examples/opargs.rs`, `examples/strings.rs`
and `examples/unknown_ops.rs` are the censuses used to settle a table entry.

## Instruction framing

```text
[opcode:u16][argLen:u16][args…]
```

Little-endian. `argLen` counts the arguments only. `0x001C` (`SetCurrentRef`) is
the exception: `1C 00 [refIdx:u16]`, four bytes, followed immediately by the
instruction it frames.

## Statements

| opcode | statement |
|---|---|
| `0x0010` | `Begin` |
| `0x0011` | `End` |
| `0x0015` | `Set` |
| `0x0016` | `If` |
| `0x0017` | `Else` |
| `0x0018` | `ElseIf` |
| `0x0019` | `EndIf` |
| `0x001C` | `SetCurrentRef` (prefix) |
| `0x001D` | `ScriptName` |
| `0x001E` | `Return` |

### `Begin`

```text
10 00 [argLen:u16] [event:u16] [blockLen:u32] [paramCount:u16] [params…]
```

- `paramCount` and the parameters exist only for the events that can take one:
  `Begin GameMode` is six argument bytes, `Begin OnDeath` eight with a count of
  zero, `Begin OnAdd player` eleven with a count of one and `72 <refIdx>`.
- An argument passed to an event that cannot take one is dropped, not encoded:
  `BEGIN ScriptEffectStart EnchARCHIMEDESEffect` writes the six bytes of a
  parameterless `Begin`.
- `blockLen` counts the body plus the `End` that closes it.
- A `Begin` parameter takes the *first* slot a name occupies in the record's
  reference list.

### `If` / `ElseIf` / `Else`

```text
16 00 [argLen:u16] [jumpOps:u16] [exprLen:u16] [expression]   If
18 00 [argLen:u16] [jumpOps:u16] [exprLen:u16] [expression]   ElseIf
17 00 02 00        [jumpOps:u16]                              Else
19 00 00 00                                                   EndIf
```

`jumpOps` is a count of *operations* in the branch's own body:
emitted instructions minus `0x1C` prefixes, plus one for each `ref` declaration
the body carries (a declaration emits no bytes but is a statement the game
counts; `int` and `float` declarations are not counted — the three units that
distinguish the two are the ones declaring a `ref` inside a branch).

Branches are emitted in source order. `else if cond` on one line writes the
`else` and drops the condition (MS01QuestScript); the next line's `if`, if any,
is what becomes the nested `if` (VCG03CampfireTriggerSCRIPT).

A stray `endif`, `else` or `elseif` that closes nothing is emitted as its own
branch head rather than refused: six shipped units do not nest, and the GECK
compiled all of them.

## Values

| tag | meaning | payload |
|---|---|---|
| `0x73` `s` | `short`/`long`/`int` variable | `u16` = the variable's `SLSD` ID |
| `0x66` `f` | `float` or `ref` variable | `u16` = the variable's `SLSD` ID |
| `0x72` `r` | one of the record's references | `u16` = 1-based slot in the record's list |
| `0x47` `G` | a global variable | `u16` = its slot; used where a number is read |
| `0x5A` `Z` | a standalone reference | `u16` = its slot |
| `0x20` | a literal | its own ASCII digits |

A variable's index is its `SLSD` first word — the ID the GECK assigned, which has
gaps — never its position in the table. The reference index space is the record's
`SCRO` and `SCRV` subrecords in record order, 1-based.

`PlayerRef` and `playerREF` resolve to the record's `Player` entry (the last one,
when a record names it twice).

A literal integer is its ASCII digits: `set V to 4500` is `05 00 | 20 | 34 35 30
30`. A negative literal is the digits *then* the postfix negate operator
`0x7E` (`set X to -1` → `20 31 20 7E`), including when the operand is a variable
(`set shoot to -shoot` → `20 73 02 00 20 7E`).

## Expressions

A condition is a run of `0x20`-prefixed tokens in postfix order, with operators
as their ASCII bytes. Precedence, as the GECK reads it — `||` binds **tighter**
than `&&`:

`||` 2 · `&&` 1 · `==` `!=` 3 · `<` `<=` `>` `>=` 4 · `+` `-` 5 · `*` `/` `%` 6

A zero-argument command used as a value is `58` (`'X'`) then `[opcode:u16]
[0x0000]`. A command *with* arguments is `58 [opcode:u16] [paramBytes:u16]
[count:u16] [args…]`, where `paramBytes` includes the count. `GetAV` uses a
different opcode in expression position (`0x100E`) than as a statement
(`0x11CD`), for all 52 corpus calls.

A dotted call is framed by `0x72 <refIdx>` before the `58`. A remote script
variable is `72 <refIdx> <tag> <the variable's own SLSD id>`; a name the target's
table does not carry is resolved in the *compiling* script's table instead
(GomorrahHotelDoorScript).

## Command arguments

```text
[opcode:u16][argLen:u16]                        command declaring no parameters
[opcode:u16][argLen:u16][count:u16][args…]      command declaring some
```

The declared type decides the encoding, before the name does:

| parameter type | encoding |
|---|---|
| `Float` | `7A` (`'z'`) + `f64` |
| `Integer`, `Int` | `6E` (`'n'`) + `i32` |
| `ActorValue` | bare `u16` code |
| `AnimGroup` | bare `u16` code |
| `Miscellaneous Stat` | bare `u16` code |
| `Sex` | bare `u16`: `Male` 0, `Female` 1 |
| `CriticalStage` | bare `u16`: `GooStart` 1, `GooEnd` 2, `DisintegrateStart` 3, `DisintegrateEnd` 4 |
| `Axis` | the ASCII letter `X`/`Y`/`Z` |
| `String` | `[len:u16] <ascii>` — the name's own text |

`ModActorValue`'s Amount is an `Integer` where `DamageAV`'s is a `Float`, and
`MoveTo`'s three offsets are `Float`. A wrong type here is a four-byte length
error that surfaces as a `blockLen` mismatch, which reads like a block bug.

### Commands with their own frame

`ShowMessage` (`0x1059`):

```text
01 00 [message] [formatCount:u16] [format values…] [u32]
```

The count word counts the message alone. A bare number after the message is not
a format value: it goes in the trailing word (`ShowMessage Msg 1` →
`01 00 | <msg> | 00 00 | 01 00 00 00`).

`ShowWarning` (`0x11B9`):

```text
[stringCount:u16] [strings…] [valueCount:u16] [values…]
```

### Commands that drop an argument

`StopCombat` (`0x1017`), `RemoveScriptPackage` (`0x1098`), `EVP` (`0x105E`),
`StopLook` (`0x105D`) and `SCAOnActor` (`0x1101`) are called with a target and
compile to `argLen` 0.

## What the compiler returns

The `SCDA` body only. The `SCHR` header's reference count, compiled size and
variable count are the record's own, and the game recomputes them on load; the
`SLSD`/`SCVR` table and the `SCRO`/`SCRV` list are the record's writer's job.
