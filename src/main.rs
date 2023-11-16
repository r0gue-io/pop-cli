mod cli;
use cli::Cli;
// mod runner;
fn main() {
    let cli = <Cli as clap::Parser>::parse();
}
