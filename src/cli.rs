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
    // Frontier,
    // Contracts,
}

#[derive(Parser, Clone)]
pub struct TemplateCmd {
    pub name: String,
    pub template: Template,
}
