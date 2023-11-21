use clap::{Parser, Subcommand};
use strum_macros::{Display, EnumIter, EnumString};
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

#[derive(Clone, Parser, Debug, Display, EnumIter, EnumString, PartialEq)]
pub enum Template {
    #[strum(serialize = "ept")]
    EPT,
    // Vanilla,
    // Kitchensink,
    #[strum(serialize = "fpt")]
    FPT,
    #[strum(serialize = "cpt")]
    Contracts,
}

#[derive(Parser, Clone)]
pub struct TemplateCmd {
    #[arg(help = "Name of the app")]
    pub name: String,
    #[arg(help = "Template to create; Options are ept, fpt, cpt")]
    pub template: Template,
}
