# Changelog

All notable changes to this project will be documented in this file.

## [0.13.0] - 2026-02-11

### 🚀 Features

- Use in-house polkadot-sdk version fetching (#775)
- Pass more flags to call command after deploying contract (#776)
- Remove salt from pop up contract (#781)
- Clean running processes (#780)
- Validate contract input (#784)
- Upgrade psvm to 0.3.1 (#792)
- Display ink-node url (#801)
- Query mappings in contracts (#779)
- Allow dev to specify the package manager in frontend templates (#802)
- Build detects sol abi (#808)
- Build debian package (#817)
- Chain metadata (#818)
- Detect running docker on runtime deterministic builds (#811)
- Allow to clean ink-node by specifying a list of ports (#874)
- Introduce a flake for nixOS users (#777)
- List templates (#880)
- Add ci for contracts (#878)
- Require signer for pop up when skipping confirmation (#884)
- Deploy local node (#882)
- Allow building contracts in workspace (#860)
- Add configuration for cargo-binstall (#891)
- Upload to ubuntu ppa (#886)
- Install pkg-config in ubuntu (#879)
- Limit to 40 the maximum list length (#893)
- Display commands (#881)
- Upgrade rust version to 1.93 (#865)
- Contract verification (#797)
- Pacman (#910)
- Add network cleanup and metadata output (#909)
- Pop-fork (#920)
- Add shell completion command (#912)
- *(pop fork)* Well-known chains, --at flag, and better detach (#930)
- Make `pop fork --endpoint` optional with interactive prompt (#928)

### 🐛 Fixes

- Parachain_lifecycle integration test (#778)
- Test compilation issues without default features (#774)
- Display full contract event (#782)
- Do not delete ink-node logs if detached (#786)
- Bug to deploy contract with salt (#787)
- Terminate local node after deployment (#791)
- *(telemetry)* Allow website_id endpoint to be overridden from hardcoded constants. (#809)
- Skip storage deposit estimation when using wallet signing (#816)
- Allow both --path and -p (#859)
- Add timeout for docker info (#861)
- Default to pop call chain (#877)
- Network selection index (#892)
- Make pop new naming consistent (#814)
- Vec for calling chains (#907)
- Handle parachain names in up network (#925)
- Remove unused import in metadata test
- *(pop-fork)* Optimize RPC performance for polkadot.js (#931)

### 🚜 Refactor

- Hide sensitive info (#798)
- Remove the code to receive the contract address from wallet signing portal (#813)
- Remove comments for local debugging in integration tests (#866)
- Allow multiple pids to be provided (#888)
- Find available random port if needed (#887)
- Rename rollup <-> chain (#883)
- Remove deprecated call contract dev flag

### 📚 Documentation

- Prepend 0.13.0 changelog
- Remove git-cliff marker

### 🎨 Styling

- Rustfmt

### 🧪 Testing

- Harden polkadot rpc for metadata test
- Remove deprecated weight-flag test

### ⚙️ Miscellaneous Tasks

- Update to 0.12.1 (#760)
- Add tests partitions (#852)
- Shared cache (#864)
- *(release)* Bump workspace deps to 0.13.0
- Min improvements to AGENTS.md (#932)

## [0.12.1] - 2025-11-15

### 🐛 Fixes

- Call txs feedback after inclusion (#755)
- Ask to execute deployment of contract in interactive flow (#759)

## [0.12.0] - 2025-11-14

### 🚀 Features

- Conditionally remove network configuration from base-parachain (#711)
- Upgrade to rust 1.91 (#708)
- Rich info for telemetry (#723)
- Scaffold UI templates (#676)
- Add command to upgrade polkadot-sdk version (#728)
- Add ink-node to pop up (#730)
- Rust 1.91.1 (#735)
- Improve eth conversion (#742)
- Remove `--dry-run` from pop call (#744)
- Bump zombienet version to 0.4.3 (#751)
- Ask chain in pop up (#754)

### 🐛 Fixes

- Release workflow in ubuntu aarch64 (#709)
- Telemetry data field (#719)
- Chain & contract manual interaction (#712)
- Remove chain spec generator for newer versions of templates (#727)
- *(install)* Support more linux flavours based on already supported distros (#731)
- Improve message (#741)
- *(pop up contract)* Signer prompted if not provided (#725)
- Add message about how to run pop install with sudo (#748)
- Add the assets precompile example (#749)
- Allow mcp to fetch the contract address upon deployment (#750)
- Remove --manual-weight flag (#752)
- tx always wait for finalization (#753)

### 🚜 Refactor

- Spawn ink-node with --tmp file (#715)
- Remove return types from commands (#726)

### ⚙️ Miscellaneous Tasks

- Release0.11.0 (#707)
- Bump rustilities (#716)
- Update passet-hub spec (#718)
- *(passet-hub)* Update spec (#736)
- Update ink! dependencies to beta.1 (#745)

## [0.11.0] - 2025-11-03

### 🚀 Features

- Always build contracts with pop up (#657)
- Upgrade to rust edition 2024 (#656)
- Do not ask for confirmation when removing all in the cache (#660)
- *(cli)* Allow to choose a chain from a list (#658)
- Build spec runtime (#647)
- Upgrade to rust 1.90 (#673)
- Read constants and storage when using `pop call chain` (#664)
- Build the chain with runtime (#671)
- *(cli)* Fetch latest release when instantiating the template (#680)
- Fetch polkadot-omni-node with pop up command (#684)
- Force specifying storage parameters if the key is composite (#697)
- Pop up chain spec cmd (#699)
- Upgrade zombienet from 0.4.1 to 0.4.2 (#703)
- Specify optional test filter (#701)
- Using ink! `v6.0.0-beta` (#645)
- Contract call read (#677)
- Add eth-rpc binary to `pop up contract` (#705)
- Remove deprecated templates (#706)

### 🐛 Fixes

- `pop install` command with rustup (#681)
- Allow `pop build spec` to specify the runtime path as argument (#687)
- Add is-relay field to `pop build spec` (#693)
- Pop call with composite types (#696)
- StripPrefixError contract and pallet creation (#702)

### 🚜 Refactor

- Remove psp example and improve pop new contract devex (#700)

### ⚙️ Miscellaneous Tasks

- Optimize ci integration tests (#653)
- *(ci)* Mark coverage status project and patch as informational only (#662)
- Publish on homebrew (#666)
- Lint PR title (#698)

## [0.10.0] - 2025-10-01

### 🚀 Features

- Allow to run a network by specifying its configuration (#594)
- Autoremove the network's base directory upon tearing down (#591)
- Add command to convert from/to Ethereum addresses (#592)
- *(cli)* Autodetect target when invoking pop call without subcommand (#609)
- *(cli)* Clearly highlight writing operations when calling smart contracts (#614)
- *(cli)* Do not ask to run with sudo for every call (#620)
- *(cli)* Follow logs of contracts node before termination (#622)
- *(cli)* Keep making calls to contract after deployment (#629)

### 🐛 Fixes

- Skip redundant question when calling a contract (#593)
- Release binary build doesn't generate all binaries (#605)
- *(cli)* Use plain output in address conversion (#608)
- Process user input for Strings and None values (#618)
- All clippy warnings (#625)
- *(ci)* Do not build docker image if exists (#638)

### 🚜 Refactor

- Clean up and speed up test suite with `nextest` (#582)
- All commands to use the CLI module (#631)

### 📚 Documentation

- Remove build command from readme that doesn't build (#637)

### 🧪 Testing

- Move some tests to integration tests (#649)

### ⚙ Miscellaneous Tasks

- Remove sp-weights from the list of dependencies (#599)
- Improve the hashing command (#603)
- Include map account as one of the quick actions in pop call (#569)
- Remove cargo deny from the CI (#627)
- Deprecate pop_evm and parity substrate-contracts-node template (#628)
- Several improvements (#648)
- `pop build spec` improvements (#640)
- Update packages to be installed (#644)

## [0.9.0] - 2025-08-01

### 🚀 Features

- *(up)* Add support for launching networks without network config files (#523)
- *(bench/pallet)* Support benchmarking multiple pallets (#547)
- Launch passet hub locally (#570)
- Add interactive mode to 'pop new' command (#578)

### 🐛 Fixes

- *(ci)* Address failing unit tests and cargo-deny issues (#548)
- *(dockerfile)* Could not launch local network (#546)
- *(contracts)* Update branch for contract template retrieval (#577)

### 🚜 Refactor

- *(common/api)* Use async-aware mutex (#541)
- Change `parachain` to `chain` for consistency (#564)

### 🧪 Testing

- *(parachains/bench)* Fix expected error in load_pallet_extrinsics test (#539)
- *(cli/try-runtime)* Update pallets in try-state test after runtime upgrade (#543)

### ⚙️ Miscellaneous Tasks

- Optimize polkavm contract jobs (#540)
- Bump version

### Build

- *(deps)* Consolidate sp-weights dependency (#538)

## [0.8.1] - 2025-05-13

### 🐛 Fixes
- Fixed template fetching for ink! v5 contracts by pointing to the correct branch.

## [0.8.0] - 2025-05-07

### 🚀 Features

- `pop build runtime` and `pop build --deterministic` (#510)
- Add experimental hashing command (#517)
- *(contracts)* Revive compatibility with feature flag (#500)

### 🐛 Fixes

- Benchmarking logger (#513)
- Wrap github rest api access in an apiclient (#530)
- *(bench)* Separate runtime binary path and runtime path (#531)

### 📚 Documentation

- Improve project documentation (#521)

### 🧪 Testing

- No default features (#522)

### ⚙️ Miscellaneous Tasks

- Source remote binary if version does not match (#516)
- Update required version of the frame-omni-bencher binary (#527)

### Build

- *(deps)* Eliminate unnecessary dependencies (#520)

### Removed

- Removed deprecated `pop test contract`, functionality is now covered by `pop test`.
- Removed deprecated `pop up contract`, functionality is now covered by `pop up`.
- Removed deprecated `pop test parachain`, functionality is now covered by `pop up`.

## [0.7.0] - 2025-04-02

### 🚀 Features

- Enable pop up without project type specification (#403)
- Update parachain templates (#297)
- Add the filter mode and password to `Cli` (#435)
- Register parachain (#404)
- Enable `pop test` without project type specification (#466)
- Benchmarking feature (#424)
- Build with try-runtime feature enabled (#476)
- Integration with deployment provider (#459)
- Try-runtime feature (#496)

### 🐛 Fixes

- Check_contracts_node handles skip_confirm (#396)
- Increase `DefaultBodyLimit` to prevent large payload failures (#409)
- Prevent recursion error with `pallet_collective` metadata (#412)
- Remove onboard.rs empty file (#433)
- Hardcoded test failing in the CI (#448)
- Display events when wallet-signing (#463)
- *(build spec)* Default bootnode prompt (#482)
- Improve build spec error messaging (#477)
- Sort releases by published_at (#489)
- Argument exists in bench commands & skip parameters flag (#494)

### 🚜 Refactor

- Check binary and prompt (#429)
- Runtime utilities & runtime feature enum (#490)

### ⚙️ Miscellaneous Tasks

- Support specify contract path input with or without -p flag (#361)
- `profile` comment in `build_parachain` (#406)
- Update cargo-deny-action@v2 (#439)
- Resolve unmaintained crate & clippy warnings (#454)
- Fix typo (#474)
- Bump versions

### Build

- *(release)* Update upload-artifact to v4 (#398)
- *(deps)* Bump openssl from 0.10.68 to 0.10.70 (#402)

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
