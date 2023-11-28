use clap::{Parser, Subcommand};
use strum_macros::{Display, EnumString};
#[derive(Parser)]
#[command(author, version, about)]
#[command(arg_required_else_help = true)]
pub struct Cli {
    #[command(subcommand)]
    pub(crate) intent: Intention,
}

#[derive(Subcommand, Clone)]
#[command(subcommand_required = true)]
pub enum Intention {
    Create(TemplateCmd),
    Pallet(PalletCmd),
}

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

#[derive(Parser, Clone)]
pub struct TemplateCmd {
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

#[derive(Parser, Clone)]
pub struct PalletCmd {
    #[arg(help = "Name of the pallet")]
    pub(crate) name: String,
    #[arg(help = "Name of authors", default_value = "Anonymous")]
    pub(crate) authors: Option<String>,
    #[arg(help = "Pallet description", default_value = "Frame Pallet")]
    pub(crate) description: Option<String>,
}
