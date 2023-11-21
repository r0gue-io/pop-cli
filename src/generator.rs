use std::{fs::OpenOptions, path::Path};

use askama::Template;

#[derive(Template)]
#[template(path = "test.templ", escape = "none")]
struct Runtime<'a> {
    code: &'a str,
    balances: bool,
}
// todo : generate directory structure

pub fn generate() {
    let hello = Runtime {
        code: r#"fn main() { 
            println!("Hello, world!"); 
        }"#,
        balances: false,
    }; // instantiate your struct
    let rendered = hello.render().unwrap();
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
