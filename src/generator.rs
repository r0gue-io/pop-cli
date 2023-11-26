//! TODO: Generators should reference files that live in the repository

use std::{fs::OpenOptions, path::Path};

use askama::Template;

// TODO: This should be coupled with Runtime in the sense that pallets part of a Runtime may need a default genesis config
#[derive(Template)]
#[template(path = "vanilla/chain_spec.templ", escape = "none")]
pub(crate) struct ChainSpec {
    pub(crate) token_symbol: String,
    pub(crate) decimals: u8,
    pub(crate) initial_endowment: String,
}
// todo : generate directory structure
// todo : This is only for development
#[allow(unused)]
pub fn generate() {
    let cs = ChainSpec {
        token_symbol: "DOT".to_owned(),
        decimals: 10,
        initial_endowment: "1u64 << 15".to_owned(),
    };
    let rendered = cs.render().unwrap();
    write_to_file(Path::new("src/x.rs"), &rendered);
}

// TODO: Check the usage of `expect`. We don't want to leave the outdir in a unhygenic state
pub(crate) fn write_to_file<'a>(path: &Path, contents: &'a str) {
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
