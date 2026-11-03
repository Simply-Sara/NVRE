"""What the Creation Kit compiler itself settles, and what it does not.

This records a black-box measurement session against the compiler that ships
with Skyrim SE (`Papyrus Compiler/PapyrusCompiler.exe`), run under Wine with a
synthetic corpus. Nothing here depends on having the exe: the *results* are
recorded so the emitter can rely on them, and the procedure is written down so
the measurement can be repeated.

Setup that works (this took several attempts):

  1. `TESV_Papyrus_Flags.flg` is NOT shipped beside the compiler. The real one
     is needed, with the grammar it documents:
         Flag Hidden 0
         { Script Property }
         Flag Conditional 1
         { Script Variable }
     A hand-written file with `flagged`/`hidden` lines is rejected with
     "missing EOF at 'hidden'" -- the grammar wants `Flag <name> <index>` and an
     optional `{ ... }` type list.

  2. The compiler needs the whole `Scripts.zip` source set as `-i` imports, not
     just the objects a script names, or every base class fails to resolve
     ("unknown type formlist"). Extracting all 14,301 sources works.

  3. `-i` must contain the directory holding the file being compiled as well,
     or it reports "unable to locate script <Name>".

What the exe answered, and what it refused
------------------------------------------

SETTLED, and now relied on by the emitter:

  * The injected `GetState`/`GotoState` pair is real and fixed. A script with no
    functions at all still compiles to those two.
  * `GetState` has an EMPTY locals table; `::State` is not a local.
  * An empty function body carries no instructions and no `::NoneVar`.
  * `native` functions carry flags=2 and no body.
  * The string table folds case: no `.pex` in the shipped corpus holds two
    entries differing only in case.
  * Types are spelled `Int Float Bool String None` in type slots.

NOT SETTLED -- the one remaining gap: the order of a state's function list.

  It is not source order, not alphabetical, not ascending string index, and not
  a consistent comparator on either index or name: the same index pair is
  emitted in both directions across different scripts. It changes when only the
  *set* changes (a name inserted earlier moves others), which is what a hash
  table does -- and `PCompiler.dll` does reference `System.Collections.Hashtable`.

  Ten synthetic probes were compiled and their exact output recorded. No
  candidate (13 string hashes x 78 capacities x two signedness masks x two
  chain orders) reproduced more than 4 of the 9 usable cases, which is chance.
  The .NET Framework `String.GetHashCode` restatement used here is a
  reconstruction, not a measurement, so a mismatch does not rule the theory out
  -- but it does mean this cannot be finished by fitting.

WHY IT IS NOT WORTH MORE ATTEMPTS:

  80.7% of the 15,635 states in the shipped corpus hold three or fewer
  functions, and for those the emitted order is one of at most six
  permutations. The bytes are already right for the overwhelming majority of
  real scripts; the ordering only costs the remaining fifth of *states*, and
  nothing at all semantically -- the VM looks functions up by name.

To repeat the measurement, put the exe's directory on PATH and run
`PapyrusCompiler.exe <file.psc> -f=<flags.flg> -i=<srcdir>;<allsources> -o=<out>`
under Wine, then read the `.pex` with `tools/interning2.py`.
"""
