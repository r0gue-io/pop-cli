use std::{fs::OpenOptions, path::Path};

use askama::Template;

#[derive(Template)]
#[template(path = "test.templ", escape = "none")]
struct Runtime<'a> {
    code: &'a str,
    balances: bool,
}

// TODO: This should be coupled with Runtime in the sense that pallets part of a Runtime may need a default genesis config
#[derive(Template)]
#[template(path = "chain_spec.templ", escape = "none")]
struct ChainSpec<'a> {
    token_symbol : &'a str,
    decimals: u8,
    initial_endowment: &'a str,
}
// todo : generate directory structure

pub fn generate() {
    let cs = ChainSpec {
        token_symbol: "DOT",
        decimals: 10,
        initial_endowment: "1u64 << 15"
    };
    let rendered = cs.render().unwrap();
    write_to_file(Path::new("src/x.rs"), &rendered);
}

fn write_to_file<'a>(path: &Path, contents: &'a str) {
    use std::io::Write;
    let mut file = OpenOptions::new()
        .write(true)
        .truncate(true)
        .open(path)
        .unwrap();
    file.write_all(contents.as_bytes()).unwrap();
    let output = std::process::Command::new("rustfmt")
        .arg(path.to_str().unwrap())
        .output()
        .expect("failed to execute rustfmt");

    if !output.status.success() {
        println!("rustfmt exited with non-zero status code.");
    }
}
