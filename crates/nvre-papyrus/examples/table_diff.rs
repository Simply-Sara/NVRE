use nvre_papyrus::emit::{Compiler, Symbols};
use nvre_papyrus::{lex, parse, PexFile};
fn main() {
    let src_path = std::env::args().nth(1).unwrap();
    let pex_path = std::env::args().nth(2).unwrap();
    let src = nvre_util::cp1252::decode(&std::fs::read(&src_path).unwrap());
    let script = parse(&lex(&src)).unwrap();
    let symbols = Symbols {
        script: script.name.clone(),
        parent: script.parent.clone().unwrap_or_default(),
        variables: Vec::new(),
        properties: Vec::new(),
        states: Vec::new(),
    };
    let mine = Compiler::new(&script, symbols).compile().unwrap();
    let game = PexFile::parse(&std::fs::read(&pex_path).unwrap()).unwrap();
    println!("{:<44} | game", "mine");
    let n = mine.strings.len().max(game.strings.len());
    let mut diffs = 0;
    for i in 0..n {
        let a = mine.strings.get(i).map(|s| s.as_str()).unwrap_or("-");
        let b = game.strings.get(i).map(|s| s.as_str()).unwrap_or("-");
        let mark = if a == b {
            " "
        } else {
            diffs += 1;
            "!"
        };
        if i < 40 || a != b {
            println!("{mark} {i:3} {a:<40} | {b}");
        }
    }
    println!(
        "\n{} of {} differ; mine {} strings, game {}",
        diffs,
        n,
        mine.strings.len(),
        game.strings.len()
    );
}
