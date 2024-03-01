use crate::engines::parachain_engine::{instantiate_template_dir, Config};
use clap::{Args, Parser};
use std::path::Path;
use strum_macros::{Display, EnumString};

#[derive(Clone, Parser, Debug, Display, EnumString, PartialEq)]
pub enum Template {
    #[strum(serialize = "Extended Parachain Template", serialize = "ept")]
    EPT,
    #[strum(serialize = "Frontier Parachain Template", serialize = "fpt")]
    FPT,
    #[strum(serialize = "Contracts Node Template", serialize = "cpt")]
    Contracts,
    #[strum(serialize = "Vanilla Parachain Template", serialize = "vanilla")]
    Vanilla,
    // Kitchensink,
}

#[derive(Args)]
pub struct NewParachainCommand {
    #[arg(help = "Name of the app. Also works as a directory path for your project")]
    pub(crate) name: String,
    #[arg(
        help = "Template to create; Options are 'ept', 'fpt', 'cpt'. Leave empty for default parachain template"
    )]
    #[arg(default_value = "vanilla")]
    pub(crate) template: Template,
    #[arg(long, short, help = "Token Symbol", default_value = "UNIT")]
    pub(crate) symbol: Option<String>,
    #[arg(long, short, help = "Token Decimals", default_value = "12")]
    pub(crate) decimals: Option<String>,
    #[arg(
        long = "endowment",
        short,
        help = "Token Endowment for dev accounts",
        default_value = "1u64 << 60"
    )]
    pub(crate) initial_endowment: Option<String>,
}

impl NewParachainCommand {
    pub(crate) fn execute(&self) -> anyhow::Result<()> {
        println!("Starting {} on `{}`!", &self.template, &self.name);
        let destination_path = Path::new(&self.name);
        instantiate_template_dir(
            &self.template,
            destination_path,
            Config {
                symbol: self.symbol.clone().expect("default values"),
                decimals: self
                    .decimals
                    .clone()
                    .expect("default values")
                    .parse::<u8>()?,
                initial_endowment: self.initial_endowment.clone().expect("default values"),
            },
        )?;
        println!("cd into {} and enjoy hacking! 🚀", &self.name);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn test_new_parachain_command_execute() -> anyhow::Result<()> {
        let command = NewParachainCommand {
            name: "test_parachain".to_string(),
            template: Template::Vanilla,
            symbol: Some("UNIT".to_string()),
            decimals: Some("12".to_string()),
            initial_endowment: Some("1u64 << 60".to_string()),
        };
        let result = command.execute();
        assert!(result.is_ok());
        
        // Clean up
        if let Err(err) = fs::remove_dir_all("test_parachain") {
            eprintln!("Failed to delete directory: {}", err);
        }
        Ok(())
    }
}
