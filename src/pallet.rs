pub fn create_pallet_template(config: PalletConfig) -> anyhow::Result<()> {
    Ok(())
}
pub struct PalletConfig {
    pub(crate) name: String,
    pub(crate) authors: String,
    pub(crate) description: String,
}
