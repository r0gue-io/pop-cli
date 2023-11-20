mod cli;
mod template;
use cli::Cli;
use std::path::Path;

fn main() -> anyhow::Result<()> {
    let cli = <Cli as clap::Parser>::parse();
    let (app_name, template) = match cli.create {
        cli::Create::Create(cli::TemplateCmd { name, template }) => (name, template),
    };
    println!("Starting {} on `{}`!", "EPT", app_name);
    let destination_path = Path::new(&app_name);
    template::instantiate_template_dir(&template, destination_path)?;
    println!("cd {app_name} and enjoy hacking!");

    Ok(())
}
