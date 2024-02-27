use clap::Args;
use crate::pallet::{TemplatePalletConfig, create_pallet_template};

#[derive(Args)]
pub struct NewPalletCommand {
    #[arg(help = "Name of the pallet", default_value = "pallet-template")]
    pub(crate) name: String,
    #[arg(short, long, help = "Name of authors", default_value = "Anonymous")]
    pub(crate) authors: Option<String>,
    #[arg(
        short,
        long,
        help = "Pallet description",
        default_value = "Frame Pallet"
    )]
    pub(crate) description: Option<String>,
    #[arg(short = 'p', long = "path", help = "Path to the pallet, [default: current directory]")]
    pub(crate) path: Option<String>,
}

impl NewPalletCommand {
    pub(crate) fn execute(&self) -> anyhow::Result<()> {
        create_pallet_template(self.path.clone(), TemplatePalletConfig {
            name: self.name.clone(),
            authors: self.authors.clone().expect("default values"),
            description: self.description.clone().expect("default values"),
        })?;
        Ok(())
    }
}