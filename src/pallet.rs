use crate::{
    generator::PalletItem,
    helpers::{clone_and_degit, resolve_pallet_path},
};
use std::fs;

pub fn create_pallet_template(path: Option<String>, config: PalletConfig) -> anyhow::Result<()> {
    let target = resolve_pallet_path(path);
    fs::create_dir(&target.join("pallet_template"))?;
    fs::create_dir(&target.join("pallet_template/src"))?;
    use crate::generator::{
        PalletBenchmarking, PalletCargoToml, PalletLib, PalletMock, PalletTests,
    };

    let pallet: Vec<Box<dyn PalletItem>> = vec![
        Box::new(PalletBenchmarking {}),
        Box::new(PalletCargoToml {
            name: config.name.clone(),
            authors: config.authors,
            description: config.description,
        }),
        Box::new(PalletLib {}),
        Box::new(PalletMock {
            pallet_name: config.name.clone(),
        }),
        Box::new(PalletTests {
            pallet_name: config.name,
        }),
    ];
    for item in pallet {
        item.execute(&target)?;
    }
    Ok(())
}
pub struct PalletConfig {
    pub(crate) name: String,
    pub(crate) authors: String,
    pub(crate) description: String,
}
