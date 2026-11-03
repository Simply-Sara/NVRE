use nvre_papyrus::{lex, parse};
fn main() {
    for p in std::env::args().skip(1) {
        let bytes = std::fs::read(&p).unwrap();
        let src = nvre_util::cp1252::decode(&bytes);
        let toks = lex(&src);
        match parse(&toks) {
            Ok(s) => println!("{p}: ok {} decls", s.decls.len()),
            Err(e) => {
                println!("{p}: {e}");
                let line: u16 = e
                    .to_string()
                    .rsplit("line ")
                    .next()
                    .and_then(|x| x.parse().ok())
                    .unwrap_or(0);
                for (i, l) in src.lines().enumerate() {
                    if (i as u16) + 1 >= line.saturating_sub(1) && (i as u16) + 1 <= line + 1 {
                        println!("   {:4}: {}", i + 1, l);
                    }
                }
            }
        }
    }
}
