#[derive(clap::Parser)]
#[command(author, version, about)]
#[command(arg_required_else_help = true)]
pub struct Cli {
    #[command(subcommand)]
    pub create: Create,
}



#[derive(clap::Subcommand)]
pub enum Create {
    /// Extended Parachain Template
    Ept,
    /// Frontier Parachain Template
    Fpt,
    #[clap(name = "solo-contracts")]
    /// Contracts Solochain Template
    SoloContracts,
    #[clap(name = "parachain-contracts")]
    /// Contracts Parachain Template
    ParaContracts,
}
