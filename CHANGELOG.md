# Changelog

All notable changes to this project will be documented in this file.

## [0.6.0] - 2024-12-19

### 🚀 Features

- Wallet integration (#371)
- Guide user to call a chain (#316)
- Guide user to call a contract (#306)
- Output events after calling a chain (#372)
- Add remark action (#387)

### 🐛 Fixes

- Build spec experience (#331)
- HRMP channels (#278)

### 🚜 Refactor

- Ensure short args consistency (#386)
- Bump fallback versions (#393)

### ⚙️ Miscellaneous Tasks

- Bump zombienet version to `v0.2.18` (#352)
- Set msrv (#385)
- Replace rococo to paseo name (#333)
- Replace codeowners (#388)

### Build

- *(deps)* Remove unused dependencies (#369)

## [0.5.0] - 2024-11-08

### 🚀 Features

- Include OpenZeppelinEVM template
- Instantiate_openzeppelin_template

### 🐛 Fixes

- Fetch stable version (#328)
- Templates errors (#329)
- Improve contract experience (#330)
- Unnnecesary API query
- Bump zombienet and insert evm_based
- Include support for v2.0.1 in generic template
- Deprecate template instantiation
- Clippy

### 🚜 Refactor

- Move extract_template_files into common crate
- Rename enum_variants_for_help macro

### ⚙️ Miscellaneous Tasks

- Merge main
- Bump zombienet-sdk version
- Bump supported version for template and add a test
- Deprecate command for change of name
- Deprecation logic
- Template_name_without_provider
- Merge main
- Upgrade zombienet-sdk
- Update evm supported version
- Fmt

## [0.4.0] - 2024-10-07

### 🚀 Features

- Adding new packages to workspace Cargo.toml automatically (#277)
- Improve pallet template generation (#261)

### 🐛 Fixes

- Update chain spec and fix fetch_latest_tag (#282)
- Remove extra 0x in outputted text when pop up (#298)
- Limit API calls when generating parachain (#299)

### 🚜 Refactor

- Update new pallet template (default) (#272)
- Move extract_template_files into common crate (#283)

### 📚 Documentation

- Add community section to README (#289)

### 🎨 Styling

- Format in ci.yml (#287)

### ⚙️ Miscellaneous Tasks

- Bump cargo-contract and subxt versions (#307)
- Add clippy checks (#281)
- Release 0.4.0 (#318)

### Build

- *(deps)* Bump zombienet-sdk (#273)

## [0.3.0] - 2024-07-26

### 🚀 Features

- *(up parachain)* Faster binary sourcing (#199)
- Clean cache (#216)
- Add paseo support (#182)
- Guide user for contract creation and add 4 contract templates (#201)
- `dry-run` flag to estimate gas (#203)
- Enable building without project type specification (#222)
- *(build parachain)* Generate specification, wasm and genesis state files (#219)
- *(contracts)* New contract templates (#249)
- Add `all` flag to `pop clean`  (#233)
- *(contract-e2e)* Auto-source substrate-contracts-node with e2e tests (#254)
- Consistency with `pop up parachains` to handle versioning for  `contracts-node` (#262)
- Pop build spec (#257)

### 🐛 Fixes

- Handle IO error if rename fails (#241)
- Readme commands (#243)
- Remove unused folders after download contracts node binary (#240)
- Check if contracts needs to be build before deploy (#246)

### 🚜 Refactor

- Standardise commands (#217)
- *(templates)* Make templates and providers generic (#226)
- Improve ux (#235)
- Upload + instantiate contract (#228)
- Ensure the UX for new contracts is consistent with the parachains. (#232)
- Improve new consistency (#245)
- Improve up ux (#248)
- Default suri (#250)

### 📚 Documentation

- Consolidate README into Docs (#223)

### 🧪 Testing

- Integration tests, doc tests and improve coverage (#242)

### ⚙️ Miscellaneous Tasks

- Set `CONTRACTS_NODE_PATH` env variable for e2e tests (#209)
- Release 0.3.0 (#244)

### Build

- *(deps)* Remove unused dependencies and fix cargo deny (#215)
- *(deps)* Bump openssl from 0.10.64 to 0.10.66 (#259)

## [0.2.0] - 2024-06-17

### 🚀 Features

- *(new)* Add next steps (#178)
- Check if build exists before deploying contract with pop up (#177)
- Add the "pop up contracts-node" command (#185)
- Add ability to run a script once network is initialized (#180)
- *(new)* Ux improvements (#191)
- Add OpenZeppelin template (#197)
- Allow user to choose build mode: debug (default) or release (#202)

### 🐛 Fixes

- Invalid .expect when parsing args with intro of pop install command (#187)
- Container ux (#186)
- Readme link to the documentation (#193)
- Improve relay command check (#212)

### 🚜 Refactor

- Use the new link for OZ templates after repo name changed (#200)
- Run substrate-contracts-node in `pop up contract` if it does not exist (#206)

### 📚 Documentation

- Add missing documentation comments for `pop_contracts` and `pop_parachain` crates (#181)

### 🎨 Styling

- Replace outdates links

### ⚙️ Miscellaneous Tasks

- V1.10.0
- Revert workspace dependencies
- Remove unused files
- Update links (#205)
- Release 0.2.0 (#210)

## [0.1.0] - 2024-05-15

### 🚀 Features

- Guide user for parachain creation (#98)
- *(up parachain)* Improve build ux (#123)
- *(help)* Possible values (#133)
- *(cli)* Integrate assets parachain template. (#132)
- *(cli)* Integrate contracts and evm parachain template. (#137)
- Pop install (#149)
- Add telemetry support (#136)
- *(telemetry)* Support DO_NOT_TRACK and CI env variables to disable telemetry (#162)

### 🐛 Fixes

- Readme link (#125)
- Retrieve templates of provider (#139)
- Update para id (#152)
- Error parsing polkadot version from github API (#142)
- Validate endowment input (#141)
- Licenses (#168)

### 📚 Documentation

- Improve descriptions (#156)
- Update docs link (#159)
- *(telemetry)* Readme explaining what and why we collect (#157)

### 🧪 Testing

- Ensure errors propagated (#143)
- Relocate integration tests (#144)
- Add more unit tests (#161)
- Mock api calls and test functionality calling an API (#164)

### ⚙️ Miscellaneous Tasks

- Use features when testing (#146)
- Update para id (#151)
- Remove telemetry for ci runs (#160)
- Add code coverage (#158)
- Licenses (#165)
- Add dependency and license checks (#170)

### Build

- *(deps)* Use git2 vendored-openssl feature (#153)
- *(deps)* Bump zombienet-sdk (#155)
- *(deps)* Update dependencies (#169)

## [0.1.0-alpha.1] - 2024-04-29

### 🚀 Features

- `pop up parachain` (#12)
- Basic contracts functionality (#17)
- Add command aliases (#19)
- Add cargo contract e2e tests for Pop CLI (#22)
- Pop up contract (#27)
- Pop build parachain (#30)
- Pop build contract: add build output (#44)
- *(new-parachain)* Add network config
- *(new-parachain)* Increase validators (#55)
- Structure for call command
- Call a smart contract
- Execute call flag
- *(pop-cli)* Pallets folder for new ones
- *(up-parachain)* Improve ux
- Init git repo (#65)
- *(up-parachain)* Enable optional verbose output (#79)
- *(new parachain)* Show polkadot version (#89)

### 🐛 Fixes

- Update dependencies (#48)
- Remove smart contract template
- *(up-parachain)* Improve command handling
- *(up-parachain)* Binary download
- *(deps)* Remove deprecated dependency (#77)
- Cargo test takes too long to test (#90)
- Fetch latest polkadot releases (#108)
- Clone when user use ssh  (#113)

### 🚜 Refactor

- Renaming, removing unused features and styling (#33)
- Improve ux (#40)
- Remove pallet template from templ files
- *(up-parachain)* Improve sourcing ux
- Separate cli frontend with backend logic (#107)

### 📚 Documentation

- Update README.md (#24)
- *(readme)* Update title (#41)
- Update readme (#45)
- Document the call command
- Improve documentation

### 🧪 Testing

- Add unit test for `pop test contract` (#70)
- Add unit test for `pop build parachain` (#81)
- Pop build contract (#83)
- Add unit test for `pop new pallet` (#84)
- Pop up parachain (#86)
- Some unit tests for call contracts and up contracts (#112)

### ⚙️ Miscellaneous Tasks

- Update manifest (#11)
- Fmt
- Fmt
- Add build/test checks (#20)
- Add rust-toolchain.toml (#73)
- Add codeowners (#80)
- Add `cargo fmt` check (#85)
- Use `thiserror` within crates (#111)
- Add license (#82)

### Build

- *(up-parachain)* Add dockerfile
- *(deps)* Bump h2 from 0.3.24 to 0.3.26 (#101)
- *(deps)* Bump rustls from 0.21.10 to 0.21.11 (#114)

### Release

- Create a release for pop-cli (#119)

<!-- generated by git-cliff -->
