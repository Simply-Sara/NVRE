use nvre_papyrus::emit::{Compiler, Symbols};
use nvre_papyrus::{lex, parse};
use std::io::Read;
fn main() {
    let a: Vec<String> = std::env::args().collect();
    let mut zip = zip::ZipArchive::new(std::fs::File::open(&a[1]).unwrap()).unwrap();
    let mut n = 0;
    for i in 0..zip.len() {
        let name = zip.by_index(i).unwrap().name().to_string();
        if !name.to_ascii_lowercase().ends_with(".psc") {
            continue;
        }
        let src = {
            let mut f = zip.by_name(&name).unwrap();
            let mut b = Vec::new();
            f.read_to_end(&mut b).unwrap();
            nvre_util::cp1252::decode(&b)
        };
        let Ok(s) = parse(&lex(&src)) else { continue };
        let symbols = Symbols {
            script: s.name.clone(),
            parent: s.parent.clone().unwrap_or_default(),
            variables: Vec::new(),
            properties: Vec::new(),
            states: Vec::new(),
        };
        if let Err(e) = Compiler::new(&s, symbols).compile() {
            println!("{}: {e}", name.rsplit('/').next().unwrap());
            n += 1;
            if n > 12 {
                break;
            }
        }
    }
}
