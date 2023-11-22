use clap::{Parser, Subcommand};
use strum_macros::{Display, EnumString};
#[derive(Parser)]
#[command(author, version, about)]
#[command(arg_required_else_help = true)]
pub struct Cli {
    #[command(subcommand)]
    pub create: Create,
}

#[derive(Subcommand, Clone)]
#[command(subcommand_required = true)]
pub enum Create {
    Create(TemplateCmd),
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
    pub name: String,
    #[arg(help = "Template to create; Options are ept, fpt, cpt")]
    #[arg(default_value = "vanilla")]
    pub template: Template,
}
