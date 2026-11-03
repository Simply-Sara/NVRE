//! Print one function, both sides, instruction by instruction.
//!
//! `showdiff` reports the first disagreement in a script and moves on, so a
//! function that differs in a later bucket is invisible. This takes the script
//! and function by name.
//!
//! ```text
//! cargo run --release --example onefunc -- Scripts.zip "Misc.bsa" abforswornbriarheartscript OnItemRemoved
//! ```

use nvre_papyrus::emit::{Compiler, Symbols};
use nvre_papyrus::pex::Function;
use nvre_papyrus::{lex, opcodes, parse, PexFile, Value};
use std::io::Read;

fn text(p: &PexFile, mine: Option<&[String]>, v: &Value) -> String {
    let s = |i: u16| match mine {
        Some(t) => t.get(i as usize).cloned().unwrap_or_default(),
        None => p.text_or_empty(i).to_string(),
    };
    match v {
        Value::None => "None".into(),
        Value::Identifier(i) => s(*i),
        Value::String(i) => format!("{:?}", s(*i)),
        Value::Int(n) => n.to_string(),
        Value::Float(f) => format!("{f}"),
        Value::Bool(b) => b.to_string(),
    }
}

fn lines(p: &PexFile, mine: Option<&[String]>, f: &Function) -> Vec<String> {
    f.instructions
        .iter()
        .map(|i| {
            let mut out = opcodes::name(i.opcode).to_string();
            for a in i.args.iter().chain(i.varargs.iter()) {
                out.push(' ');
                out.push_str(&text(p, mine, a));
            }
            out
        })
        .collect()
}

fn main() {
    let a: Vec<String> = std::env::args().collect();
    let want_script = a[3].to_ascii_lowercase();
    let want_fn = a[4].to_ascii_lowercase();

    let mut zip = zip::ZipArchive::new(std::fs::File::open(&a[1]).unwrap()).unwrap();
    let mut archive = bsa105::Archive::open(&a[2]).unwrap();

    // The source, by leaf name.
    let mut src = None;
    for i in 0..zip.len() {
        let name = zip.by_index(i).unwrap().name().to_string();
        let leaf = name
            .rsplit('/')
            .next()
            .unwrap()
            .trim_end_matches(".psc")
            .to_ascii_lowercase();
        if leaf == want_script {
            let mut f = zip.by_name(&name).unwrap();
            let mut b = Vec::new();
            f.read_to_end(&mut b).unwrap();
            src = Some(nvre_util::cp1252::decode(&b));
            break;
        }
    }
    let Some(src) = src else {
        eprintln!("no source for {want_script}");
        return;
    };
    let script = parse(&lex(&src)).unwrap();
    let symbols = Symbols {
        script: script.name.clone(),
        parent: script.parent.clone().unwrap_or_default(),
        variables: Vec::new(),
        properties: Vec::new(),
        states: Vec::new(),
    };
    let c = Compiler::new(&script, symbols).compile().unwrap();

    let bytes = archive
        .extract_by_path(&format!("scripts\\{want_script}.pex"))
        .unwrap();
    let g = PexFile::parse(&bytes).unwrap();

    let mine = c.states.iter().flat_map(|(_, fs)| fs.iter()).find(|f| {
        c.strings
            .get(f.name as usize)
            .map(|s| s.eq_ignore_ascii_case(&want_fn))
            .unwrap_or(false)
    });
    let Some(mine) = mine else {
        eprintln!("my emitter produced no function {want_fn}");
        return;
    };
    let theirs = g
        .objects
        .iter()
        .flat_map(|o| o.states.iter())
        .flat_map(|s| s.functions.iter())
        .find(|f| g.text_or_empty(f.name).eq_ignore_ascii_case(&want_fn));
    let Some(theirs) = theirs else {
        eprintln!("no shipped function {want_fn}");
        return;
    };

    println!(
        "game locals: {:?}",
        theirs
            .locals
            .iter()
            .map(|t| format!(
                "{}:{}",
                g.text_or_empty(t.name),
                g.text_or_empty(t.type_name)
            ))
            .collect::<Vec<_>>()
    );
    println!(
        "mine locals: {:?}",
        mine.locals
            .iter()
            .map(|t| format!(
                "{}:{}",
                c.strings.get(t.name as usize).cloned().unwrap_or_default(),
                c.strings
                    .get(t.type_name as usize)
                    .cloned()
                    .unwrap_or_default()
            ))
            .collect::<Vec<_>>()
    );
    println!(
        "game params: {:?}",
        theirs
            .params
            .iter()
            .map(|t| g.text_or_empty(t.name).to_string())
            .collect::<Vec<_>>()
    );
    println!();
    let gt = lines(&g, None, theirs);
    let mt = lines(&g, Some(&c.strings), mine);
    for i in 0..gt.len().max(mt.len()) {
        let gi = gt.get(i).map(String::as_str).unwrap_or("<absent>");
        let mi = mt.get(i).map(String::as_str).unwrap_or("<absent>");
        let mark = if gi == mi { "  " } else { "!!" };
        println!("{mark}{i:3} game {gi}");
        println!("    {i:3} mine {mi}");
    }
}
