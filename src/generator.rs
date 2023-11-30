//! TODO: Generators should reference files that live in the repository

use std::path::{Path, PathBuf};

use askama::Template;

use crate::helpers::write_to_file;

// TODO: This should be coupled with Runtime in the sense that pallets part of a Runtime may need a default genesis config
#[derive(Template)]
#[template(path = "vanilla/chain_spec.templ", escape = "none")]
pub(crate) struct ChainSpec {
    pub(crate) token_symbol: String,
    pub(crate) decimals: u8,
    pub(crate) initial_endowment: String,
}

#[derive(Template)]
#[template(path = "pallet/Cargo.templ", escape = "none")]
pub(crate) struct PalletCargoToml {
    pub(crate) name: String,
    pub(crate) authors: String,
    pub(crate) description: String,
}
#[derive(Template)]
#[template(path = "pallet/src/benchmarking.rs.templ", escape = "none")]
pub(crate) struct PalletBenchmarking {}
#[derive(Template)]
#[template(path = "pallet/src/lib.rs.templ", escape = "none")]
pub(crate) struct PalletLib {}
#[derive(Template)]
#[template(path = "pallet/src/mock.rs.templ", escape = "none")]
pub(crate) struct PalletMock {
    pub(crate) module: String,
}
#[derive(Template)]
#[template(path = "pallet/src/tests.rs.templ", escape = "none")]
pub(crate) struct PalletTests {
    pub(crate) module: String,
}

// todo : generate directory structure
// todo : This is only for development
#[allow(unused)]
pub fn generate() {
    let cs = ChainSpec {
        token_symbol: "DOT".to_owned(),
        decimals: 10,
        initial_endowment: "1u64 << 15".to_owned(),
    };
    let rendered = cs.render().unwrap();
    write_to_file(Path::new("src/x.rs"), &rendered);
}

pub trait PalletItem {
    fn execute(&self, root: &PathBuf) -> anyhow::Result<()>;
}

impl PalletItem for PalletTests {
    fn execute(&self, root: &PathBuf) -> anyhow::Result<()> {
        let rendered = self.render()?;
        write_to_file(&root.join("src/tests.rs"), &rendered);
        Ok(())
    }
}
impl PalletItem for PalletMock {
    fn execute(&self, root: &PathBuf) -> anyhow::Result<()> {
        let rendered = self.render()?;
        write_to_file(&root.join("src/mock.rs"), &rendered);
        Ok(())
    }
}
impl PalletItem for PalletLib {
    fn execute(&self, root: &PathBuf) -> anyhow::Result<()> {
        let rendered = self.render()?;
        write_to_file(&root.join("src/lib.rs"), &rendered);
        Ok(())
    }
}
impl PalletItem for PalletBenchmarking {
    fn execute(&self, root: &PathBuf) -> anyhow::Result<()> {
        let rendered = self.render()?;
        write_to_file(&root.join("src/benchmarking.rs"), &rendered);
        Ok(())
    }
}
impl PalletItem for PalletCargoToml {
    fn execute(&self, root: &PathBuf) -> anyhow::Result<()> {
        let rendered = self.render()?;
        write_to_file(&root.join("Cargo.toml"), &rendered);
        Ok(())
    }
}
