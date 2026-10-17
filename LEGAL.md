# Legal notice

NVRE is a free, open-source plugin editor for *Fallout: New Vegas*. It reads the
data files of your own lawfully purchased copy of the game and writes new plugins
(`.esp`); it never modifies your installation.

It is not affiliated with, endorsed by, sponsored by or otherwise connected to
Bethesda Softworks, ZeniMax Media, Microsoft, Obsidian Entertainment or Valve.

- NVRE contains no code from the original game, from its editor (the G.E.C.K.) or
  from the Gamebryo engine.
- NVRE never writes into the game folder or into
  `Documents\My Games\FalloutNV`. The game's `Data` folder is read-only input,
  and saving writes a new `.esp` wherever you ask.
- NVRE does not circumvent copy protection.
- This repository ships no game content: no plugins or archives (`.esm`, `.esp`,
  `.bsa`), no meshes, textures or animations (`.nif`, `.dds`, `.kf`), no audio
  (`.ogg`, `.wav`, `.mp3`), no saves and no executables. `.gitignore` blocks these
  file types so they cannot be committed by accident.

*Fallout*, *Fallout: New Vegas* and related names and logos are trademarks or
registered trademarks of Bethesda Softworks LLC, a ZeniMax Media company, in the
United States and/or other countries. *Fallout: New Vegas* was developed by
Obsidian Entertainment. These names are used only to say what NVRE is compatible
with; their use does not imply any affiliation or endorsement, and the project
claims no rights in them.

## Licence

NVRE is free software, licensed under the **GNU General Public License, version 3
or (at your option) any later version** (`GPL-3.0-or-later`), matching
[NVForever](../NVForever), whose format crates NVRE is built on. The full text is
in [LICENSE](LICENSE).

This licence covers NVRE's own code and documentation. It does not cover, and
grants no rights in, *Fallout: New Vegas* or its data files, which remain the
property of their owners and subject to the terms under which you bought the game.

Third-party Rust crates that NVRE depends on keep their own licences (MIT,
Apache-2.0 and similar permissive licences).
