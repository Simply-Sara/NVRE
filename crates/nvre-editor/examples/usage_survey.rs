//! How expensive is it to count references to every base object?
//!
//! Both new columns need a walk of every record in the load order: Count
//! needs each placed reference's base object, and Users needs every FormID
//! field in every record. This measures that walk against a real master
//! before any of it is wired into the object window, so the cost is known
//! rather than guessed at.
//!
//!   NVRE_DATA=<Data> cargo run -p nvre-editor --example usage_survey

use nvre_editor::session::Session;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let data = std::env::var("NVRE_DATA")?;
    let dir = std::path::Path::new(&data);
    let master = dir.join("FalloutNV.esm");
    let schema_dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../schemas/fnv");

    let t0 = std::time::Instant::now();
    let mut s = Session::open(&master, &schema_dir, vec!["FalloutNV.esm".to_string()])?;
    let open_ms = t0.elapsed().as_millis();
    println!("session open: {open_ms} ms");

    // --- Count: base objects and their placed copies ---
    //
    // A reference's base is its NAME subrecord's FormID, mapped to the load
    // order. The world index already knows which reference is in which cell,
    // so every reference in the game is `index.refs`.
    let t = std::time::Instant::now();
    let mut counts: std::collections::HashMap<u32, usize> = std::collections::HashMap::new();
    let mut scanned_refs = 0usize;
    for (&fid, entry) in &s.index.refs {
        if !matches!(
            entry.sig,
            [b'R', b'E', b'F', b'R']
                | [b'A', b'C', b'R', b'E']
                | [b'A', b'C', b'H', b'R']
                | [b'P', b'M', b'R', b'E']
                | [b'P', b'G', b'R', b'E']
                | [b'P', b'H', b'Z', b'D']
        ) {
            continue;
        }
        scanned_refs += 1;
        // The base object, read the same way the references table does.
        let base = s
            .resolved_record(fid)
            .and_then(|r| nvre_edit::document::value_of(&s.schema, &r, b"NAME"))
            .and_then(|v| v.as_str().map(str::to_string))
            .and_then(|b| u32::from_str_radix(b.strip_prefix("0x")?, 16).ok());
        if let Some(base) = base {
            *counts.entry(base).or_insert(0) += 1;
        }
    }
    let count_ms = t.elapsed().as_millis();
    println!(
        "count: {} references scanned in {count_ms} ms, {} base objects carry copies",
        scanned_refs,
        counts.len()
    );
    let top: Vec<_> = {
        let mut v: Vec<(u32, usize)> = counts.iter().map(|(&k, &v)| (k, v)).collect();
        v.sort_by(|a, b| b.1.cmp(&a.1));
        v.truncate(5);
        v
    };
    for (fid, n) in top {
        let name = s
            .resolved_record(fid)
            .and_then(|r| nvre_edit::document::edid_of(&r))
            .unwrap_or_else(|| format!("{fid:08X}"));
        println!("  {name}: {n}");
    }

    // --- Users: every record that names a FormID anywhere in its fields ---
    //
    // A record "uses" an object when one of its fields is that object's
    // FormID. The scan is over every record in the load order, reading the raw
    // bytes at the positions the schema says hold a FormID rather than decoding
    // the subrecord whole: the decode is what makes this expensive.
    let t = std::time::Instant::now();
    let mut users: std::collections::HashMap<u32, usize> = std::collections::HashMap::new();
    let mut records_scanned = 0usize;
    let mut subs_scanned = 0usize;
    let mut fids_found = 0usize;
    for (pi, _plugin) in s.load_order().unwrap().plugins.iter().enumerate() {
        let recs = &s.load_order().unwrap().records[pi];
        for r in recs {
            records_scanned += 1;
            let Ok(subs) = r.subrecords() else { continue };
            for sub in &subs {
                subs_scanned += 1;
                let Some(def) = s.schema.lookup(&r.header.sig, &sub.sig) else {
                    continue;
                };
                // Read only what the schema says holds a FormID.
                if !type_has_formid(&s.schema, &def.ty) {
                    continue;
                }
                for f in scan_formids(&s.schema, &def.ty, &sub.data) {
                    fids_found += 1;
                    if let Some(g) = s.load_order().unwrap().to_global(pi, f) {
                        *users.entry(g).or_insert(0) += 1;
                    }
                }
            }
        }
    }
    let users_ms = t.elapsed().as_millis();
    println!(
        "users: {records_scanned} records, {subs_scanned} subrecords, {fids_found} FormIDs, {users_ms} ms; {} objects are used",
        users.len()
    );
    let top: Vec<_> = {
        let mut v: Vec<(u32, usize)> = users.iter().map(|(&k, &v)| (k, v)).collect();
        v.sort_by(|a, b| b.1.cmp(&a.1));
        v.truncate(5);
        v
    };
    for (fid, n) in top {
        let name = s
            .resolved_record(fid)
            .and_then(|r| nvre_edit::document::edid_of(&r))
            .unwrap_or_else(|| format!("{fid:08X}"));
        println!("  {name}: {n}");
    }
    Ok(())
}

/// Whether a decoded type can hold a FormID anywhere inside it.
///
/// The schema has two parallel shapes — [`nvre_esm::schema::Type`] for a
/// subrecord's own layout, and [`nvre_esm::schema::TypeDef`] for a named
/// type's — so both are walked, and a named type is followed through the
/// schema's own tables. A `formid` anywhere inside either is enough: the
/// decoded value is a tree of `0x...` strings, so one formid field means the
/// whole subrecord is worth decoding.
fn type_has_formid(schema: &nvre_esm::Schema, ty: &nvre_esm::schema::Type) -> bool {
    use nvre_esm::schema::{Prim, Type};
    match ty {
        Type::Prim(p) => matches!(p, Prim::FormId),
        Type::Named(n) => schema
            .types
            .get(n)
            .or_else(|| schema.common.get(n))
            .is_some_and(|t| typedef_has_formid(schema, t)),
        Type::Inline(fields) => fields.iter().any(|f| type_has_formid(schema, &f.ty)),
        Type::Repeat(inner) => type_has_formid(schema, inner),
        Type::Array { of, .. } => type_has_formid(schema, of),
        Type::Union(variants) => variants.iter().any(|v| type_has_formid(schema, &v.ty)),
        Type::ValueUnion(vu) => vu.variants.iter().any(|v| type_has_formid(schema, &v.ty)),
        _ => false,
    }
}

/// Whether a named type can hold a FormID anywhere inside it.
///
/// A named type is a struct, an enum, a set of flags, or a union — and an
/// alias for another type, which is followed. An enum or a set of flags holds
/// integers, so only the struct's fields and the unions' variants can name a
/// record.
fn typedef_has_formid(schema: &nvre_esm::Schema, ty: &nvre_esm::schema::TypeDef) -> bool {
    let mut any = ty.fields.iter().any(|f| type_has_formid(schema, &f.ty));
    any |= ty.union.iter().any(|v| type_has_formid(schema, &v.ty));
    if let Some(alias) = &ty.alias {
        any |= type_has_formid(schema, alias);
    }
    if let Some(vu) = &ty.value_union {
        any |= vu.variants.iter().any(|v| type_has_formid(schema, &v.ty));
    }
    if let Some(pu) = &ty.peek_union {
        any |= pu.variants.iter().any(|v| type_has_formid(schema, &v.ty));
    }
    any
}

/// Decode one subrecord against its record's schema.
fn decode(
    schema: &nvre_esm::Schema,
    rec: &nvre_esm::Record,
    sub: &nvre_esm::Subrecord,
) -> Option<serde_json::Value> {
    let f = nvre_world::fields::Fields::new(rec, 0, schema).ok()?;
    f.decode(sub)
}

/// Every file-local FormID a subrecord's raw bytes hold, at the positions the
/// schema says.
///
/// The walk is over the schema's own layout rather than over decoded JSON: a
/// `formid` is four bytes and nothing else, so reading them directly avoids
/// the allocation and the string round-trip that make a decode cost what it
/// does. An array or a union's count comes from the schema, not from the data,
/// which is why a length-prefixed array has to be read a word at a time.
fn scan_formids(schema: &nvre_esm::Schema, ty: &nvre_esm::schema::Type, data: &[u8]) -> Vec<u32> {
    let mut out = Vec::new();
    let mut pos = 0usize;
    scan(schema, ty, data, &mut pos, &mut out);
    out
}

/// Read one type from `data` at `pos`, advancing `pos` past what it read.
///
/// A field the schema says is optional is read only when the bytes allow it,
/// and one whose count comes from a sibling's value is read as many times as
/// that value says — which is the same rule the decoder follows, because the
/// bytes are the decoder's.
fn scan(
    schema: &nvre_esm::Schema,
    ty: &nvre_esm::schema::Type,
    data: &[u8],
    pos: &mut usize,
    out: &mut Vec<u32>,
) {
    use nvre_esm::schema::{Prim, Type};
    match ty {
        Type::Prim(Prim::FormId) => {
            if let Some(w) = data.get(*pos..*pos + 4) {
                let v = u32::from_le_bytes(w.try_into().unwrap());
                if v != 0 {
                    out.push(v);
                }
            }
            *pos += 4;
        }
        Type::Prim(p) => *pos += prim_size(*p),
        Type::Named(n) => {
            if let Some(t) = schema.types.get(n).or_else(|| schema.common.get(n)) {
                scan_def(schema, t, data, pos, out);
            }
        }
        Type::Inline(fields) => {
            for f in fields {
                scan(schema, &f.ty, data, pos, out);
            }
        }
        Type::Repeat(inner) => {
            while *pos < data.len() {
                let before = *pos;
                scan(schema, inner, data, pos, out);
                if *pos <= before {
                    break;
                }
            }
        }
        Type::Array { of, count } => {
            // The count is a fixed number, a sibling's value, or the set bits
            // of one — and, when it is a `Prefix`, a word in the data before
            // the elements.
            let n = match count {
                nvre_esm::schema::ArrayCount::Fixed(n) => Some(*n),
                nvre_esm::schema::ArrayCount::Prefix(p) => {
                    let sz = prim_size(*p);
                    let Some(w) = data.get(*pos..*pos + sz) else {
                        return;
                    };
                    *pos += sz;
                    Some(match p {
                        Prim::U8 | Prim::I8 => u32::from(w[0]),
                        Prim::U16 | Prim::I16 => {
                            u32::from(u16::from_le_bytes(w.try_into().unwrap()))
                        }
                        _ => u32::from_le_bytes(w[..4].try_into().unwrap()),
                    } as usize)
                }
                // A count from a sibling field is not tracked here: the probe
                // measures the walk, and a struct's fields are read in order,
                // so a sibling read earlier is the one that counts.
                _ => None,
            };
            match n {
                Some(n) => {
                    for _ in 0..n {
                        scan(schema, of, data, pos, out);
                    }
                }
                // No count: the elements fill what is left, which for a FormID
                // is every remaining word.
                None => {
                    while *pos + 4 <= data.len() {
                        scan(schema, of, data, pos, out);
                    }
                }
            }
        }
        Type::Union(variants) => {
            // The variant whose size matches what is left, the way the decoder
            // chooses one.
            let left = data.len() - *pos;
            if let Some(v) = variants
                .iter()
                .find(|v| v.size == Some(left))
                .or_else(|| variants.last())
            {
                scan(schema, &v.ty, data, pos, out);
            }
        }
        Type::ValueUnion(vu) => {
            if let Some(v) = vu.variants.get(vu.default) {
                scan(schema, &v.ty, data, pos, out);
            }
        }
        _ => {}
    }
}

fn scan_def(
    schema: &nvre_esm::Schema,
    ty: &nvre_esm::schema::TypeDef,
    data: &[u8],
    pos: &mut usize,
    out: &mut Vec<u32>,
) {
    for f in &ty.fields {
        scan(schema, &f.ty, data, pos, out);
    }
    for v in &ty.union {
        scan(schema, &v.ty, data, pos, out);
    }
    if let Some(alias) = &ty.alias {
        scan(schema, alias, data, pos, out);
    }
    let _ = out;
}

/// How many bytes a primitive occupies.
///
/// A string's length is in the data, so it reads as zero here and is skipped
/// by the caller: a walk that cannot read a length cannot walk past the
/// string, and the fields after it would be read from the wrong offset.
fn prim_size(p: nvre_esm::schema::Prim) -> usize {
    use nvre_esm::schema::Prim;
    match p {
        Prim::U8 | Prim::I8 | Prim::Bool => 1,
        Prim::U16 | Prim::I16 => 2,
        Prim::U32 | Prim::I32 | Prim::FormId | Prim::F32 | Prim::PeekU32 | Prim::Localized => 4,
        Prim::U64 | Prim::I64 | Prim::F64 => 8,
        _ => 0,
    }
}
