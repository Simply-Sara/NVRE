//! Read every BSA v105 in the user's Skyrim SE install and report what the
//! reader sees: file counts, any layout anomaly, and that extraction works.
use bsa105::Archive;
fn main() {
    let dir = std::env::args().nth(1).expect("usage: walk <Data dir>");
    let mut paths: Vec<_> = std::fs::read_dir(&dir)
        .unwrap()
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| {
            p.extension()
                .map(|x| x.eq_ignore_ascii_case("bsa"))
                .unwrap_or(false)
        })
        .collect();
    paths.sort();
    println!("{} archives", paths.len());
    let mut grand_total = 0u64;
    let mut bad = 0;
    for p in &paths {
        let name = p.file_name().unwrap().to_string_lossy().into_owned();
        match Archive::open(p) {
            Err(e) => {
                println!("{name:28} ERROR {e}");
                bad += 1;
            }
            Ok(mut a) => {
                let files: u64 = a.folders.iter().map(|f| f.count as u64).sum();
                grand_total += files;
                // Extract the first three files of each archive. Clone the paths
                // first: `extract` needs `&mut self` and `entries` borrows.
                let sample_paths: Vec<(String, usize, usize)> = a
                    .entries()
                    .take(3)
                    .map(|e| (e.path(), e.folder_index, e.file_index))
                    .collect();
                let mut sample = String::new();
                for (path, fi, ri) in sample_paths {
                    let rec = a.folders[fi].files[ri].clone();
                    match a.extract(&rec) {
                        Ok(b) => sample.push_str(&format!("{path}={}B ", b.len())),
                        Err(er) => {
                            sample.push_str(&format!("{path}=ERR({er}) "));
                            bad += 1;
                        }
                    }
                }
                let anomalies = a
                    .warnings
                    .iter()
                    .map(|w| format!("{}:{}", w.code, w.detail))
                    .collect::<Vec<_>>()
                    .join(" | ");
                println!(
                    "{name:28} v{} folders={:5} files={files:6} {sample}",
                    a.header.version,
                    a.folders.len()
                );
                if !anomalies.is_empty() {
                    println!("{:>28} ANOMALIES: {anomalies}", "");
                }
            }
        }
    }
    println!("total files {grand_total}, failures {bad}");
    assert_eq!(bad, 0, "failures above");
}
