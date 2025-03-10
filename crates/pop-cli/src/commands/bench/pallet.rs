// SPDX-License-Identifier: GPL-3.0

use crate::{
	cli::{
		self,
		traits::{Confirm, Input, MultiSelect, Select},
	},
	common::{
		bench::{
			check_omni_bencher_and_prompt, ensure_runtime_binary_exists,
			guide_user_to_select_genesis_policy, guide_user_to_select_genesis_preset,
		},
		prompt::display_message,
	},
};
use clap::Args;
use cliclack::spinner;
use pop_common::{get_relative_or_absolute_path, Profile};
use pop_parachains::{
	generate_benchmarks, get_preset_names, load_pallet_extrinsics, GenesisBuilderPolicy,
	PalletExtrinsicsRegistry, GENESIS_BUILDER_DEV_PRESET,
};
use std::{
	collections::HashMap,
	env::current_dir,
	path::{Path, PathBuf},
};
use strum::{EnumMessage, IntoEnumIterator};
use strum_macros::{EnumIter, EnumMessage as EnumMessageDerive};

const ALL_SELECTED: &str = "*";

#[derive(Args)]
pub(crate) struct BenchmarkPallet {
	/// Select a pallet to benchmark, or `*` for all (in which case `extrinsic` must be `*`).
	#[arg(short, long, value_parser = parse_pallet_name, default_value_if("all", "true", Some("*".into())))]
	pub pallet: Option<String>,

	/// Select an extrinsic inside the pallet to benchmark, or `*` for all.
	#[arg(short, long, default_value_if("all", "true", Some("*".into())))]
	pub extrinsic: Option<String>,

	/// Comma separated list of pallets that should be excluded from the benchmark.
	#[arg(long, value_parser, num_args = 1.., value_delimiter = ',')]
	pub exclude_pallets: Vec<String>,

	/// Run benchmarks for all pallets and extrinsics.
	///
	/// This is equivalent to running `--pallet * --extrinsic *`.
	#[arg(long)]
	pub all: bool,

	/// Select how many samples we should take across the variable components.
	#[arg(short, long, default_value_t = 50)]
	pub steps: u32,

	/// Indicates lowest values for each of the component ranges.
	#[arg(long = "low", value_delimiter = ',')]
	pub lowest_range_values: Vec<u32>,

	/// Indicates highest values for each of the component ranges.
	#[arg(long = "high", value_delimiter = ',')]
	pub highest_range_values: Vec<u32>,

	/// Select how many repetitions of this benchmark should run from within the wasm.
	#[arg(short, long, default_value_t = 20)]
	pub repeat: u32,

	/// Select how many repetitions of this benchmark should run from the client.
	///
	/// NOTE: Using this alone may give slower results, but will afford you maximum Wasm memory.
	#[arg(long, default_value_t = 1)]
	pub external_repeat: u32,

	/// Print the raw results in JSON format.
	#[arg(long = "json")]
	pub json_output: bool,

	/// Write the raw results in JSON format into the given file.
	#[arg(long, conflicts_with = "json_output")]
	pub json_file: Option<PathBuf>,

	/// Don't print the median-slopes linear regression analysis.
	#[arg(long)]
	pub no_median_slopes: bool,

	/// Don't print the min-squares linear regression analysis.
	#[arg(long)]
	pub no_min_squares: bool,

	/// Output the benchmarks to a Rust file at the given path.
	#[arg(long)]
	pub output: Option<PathBuf>,

	/// Path to Handlebars template file used for outputting benchmark results. (Optional)
	#[arg(long)]
	pub template: Option<PathBuf>,

	/// Which analysis function to use when outputting benchmarks:
	/// * min-squares (default)
	/// * median-slopes
	/// * max (max of min squares and median slopes for each value)
	#[arg(long)]
	pub output_analysis: Option<String>,

	/// Which analysis function to use when analyzing measured proof sizes.
	#[arg(long, default_value("median-slopes"))]
	pub output_pov_analysis: Option<String>,

	/// Set the heap pages while running benchmarks. If not set, the default value from the client
	/// is used.
	#[arg(long)]
	pub heap_pages: Option<u64>,

	/// Disable verification logic when running benchmarks.
	#[arg(long)]
	pub no_verify: bool,

	/// Display and run extra benchmarks that would otherwise not be needed for weight
	/// construction.
	#[arg(long)]
	pub extra: bool,

	/// Optional runtime blob to use instead of the one from the genesis config.
	#[arg(long)]
	pub runtime: Option<PathBuf>,

	/// Do not fail if there are unknown but also unused host functions in the runtime.
	#[arg(long)]
	pub allow_missing_host_functions: bool,

	/// How to construct the genesis state.
	#[arg(long, alias = "genesis-builder-policy")]
	pub genesis_builder: Option<GenesisBuilderPolicy>,

	/// The preset that we expect to find in the GenesisBuilder runtime API.
	///
	/// This can be useful when a runtime has a dedicated benchmarking preset instead of using the
	/// default one.
	#[arg(long, default_value = GENESIS_BUILDER_DEV_PRESET)]
	pub genesis_builder_preset: String,

	/// Limit the memory the database cache can use.
	#[arg(long = "db-cache", value_name = "MiB", default_value_t = 1024)]
	pub database_cache_size: u32,

	/// List and print available benchmarks in a csv-friendly format.
	#[arg(long)]
	pub list: bool,

	/// If enabled, the storage info is not displayed in the output next to the analysis.
	///
	/// This is independent of the storage info appearing in the *output file*. Use a Handlebar
	/// template for that purpose.
	#[arg(long)]
	pub no_storage_info: bool,

	/// The assumed default maximum size of any `StorageMap`.
	///
	/// When the maximum size of a map is not defined by the runtime developer,
	/// this value is used as a worst case scenario. It will affect the calculated worst case
	/// PoV size for accessing a value in a map, since the PoV will need to include the trie
	/// nodes down to the underlying value.
	#[clap(long = "map-size", default_value = "1000000")]
	pub worst_case_map_values: u32,

	/// Adjust the PoV estimation by adding additional trie layers to it.
	///
	/// This should be set to `log16(n)` where `n` is the number of top-level storage items in the
	/// runtime, eg. `StorageMap`s and `StorageValue`s. A value of 2 to 3 is usually sufficient.
	/// Each layer will result in an additional 495 bytes PoV per distinct top-level access.
	/// Therefore multiple `StorageMap` accesses only suffer from this increase once. The exact
	/// number of storage items depends on the runtime and the deployed pallets.
	#[clap(long, default_value = "2")]
	pub additional_trie_layers: u8,

	/// Do not enable proof recording during time benchmarking.
	///
	/// By default, proof recording is enabled during benchmark execution. This can slightly
	/// inflate the resulting time weights. For parachains using PoV-reclaim, this is typically the
	/// correct setting. Chains that ignore the proof size dimension of weight (e.g. relay chain,
	/// solo-chains) can disable proof recording to get more accurate results.
	#[arg(long)]
	disable_proof_recording: bool,

	/// If this is set to true, no parameter menu pops up
	#[arg(long = "skip")]
	skip_menu: bool,

	/// Automatically source the needed binary required without prompting for confirmation.
	#[clap(short = 'y', long)]
	skip_confirm: bool,
}

impl Default for BenchmarkPallet {
	fn default() -> Self {
		Self {
			pallet: None,
			extrinsic: None,
			exclude_pallets: vec![],
			all: false,
			steps: 50,
			lowest_range_values: vec![],
			highest_range_values: vec![],
			repeat: 20,
			external_repeat: 1,
			json_output: false,
			json_file: None,
			no_median_slopes: false,
			no_min_squares: false,
			output: None,
			template: None,
			output_analysis: None,
			output_pov_analysis: Some("median-slopes".to_string()),
			heap_pages: None,
			no_verify: false,
			extra: false,
			runtime: None,
			allow_missing_host_functions: false,
			genesis_builder: None,
			genesis_builder_preset: GENESIS_BUILDER_DEV_PRESET.to_string(),
			database_cache_size: 1024,
			list: false,
			no_storage_info: false,
			worst_case_map_values: 1000000,
			additional_trie_layers: 2,
			disable_proof_recording: false,
			skip_menu: false,
			skip_confirm: false,
		}
	}
}

impl BenchmarkPallet {
	pub async fn execute(&mut self, cli: &mut impl cli::traits::Cli) -> anyhow::Result<()> {
		// If `all` is provided, we override the value of `pallet` and `extrinsic` to select all.
		if self.all {
			self.pallet = Some(ALL_SELECTED.to_string());
			self.extrinsic = Some(ALL_SELECTED.to_string());
			self.all = false;
		}

		let mut registry: PalletExtrinsicsRegistry = HashMap::default();

		cli.intro(if self.list {
			"Listing available pallets and extrinsics"
		} else {
			"Benchmarking your pallets"
		})?;
		cli.warning(
			"NOTE: the `pop bench pallet` is not yet battle tested - double check the results.",
		)?;

		// No runtime path provided, auto-detect the runtime WASM binary. If not found, build
		// the runtime.
		if self.runtime.is_none() {
			match ensure_runtime_binary_exists(cli, &get_current_directory(), &Profile::Release) {
				Ok(runtime_binary_path) => self.runtime = Some(runtime_binary_path),
				Err(e) => {
					return display_message(&e.to_string(), false, cli);
				},
			}
		}

		if self.list {
			// Without overriding the genesis builder policy, listing will fail for a runtime
			// that is not built with the `runtime-benchmarks` feature.
			self.genesis_builder = Some(GenesisBuilderPolicy::None);
			if let Err(e) = self.run() {
				return display_message(&e.to_string(), false, cli);
			}
			return display_message("All pallets and extrinsics listed!", true, cli);
		}

		// No genesis builder, prompts user to select the genesis builder policy.
		if self.genesis_builder.is_none() {
			let runtime_path = self.runtime()?.clone();
			let preset_names = get_preset_names(&runtime_path).unwrap_or_default();
			// Determine policy based on preset availability.
			let policy = if preset_names.is_empty() {
				GenesisBuilderPolicy::None
			} else {
				guide_user_to_select_genesis_policy(cli, &self.genesis_builder)?
			};
			self.genesis_builder = Some(policy);

			// If the policy requires a preset, prompt the user to select one.
			if policy == GenesisBuilderPolicy::Runtime {
				self.genesis_builder_preset = guide_user_to_select_genesis_preset(
					cli,
					&runtime_path,
					&self.genesis_builder_preset,
				)?;
			}
		}

		// No pallet provided, prompts user to select the pallet fetched from runtime.
		if self.pallet.is_none() {
			self.update_pallets(cli, &mut registry).await?;
		}
		// No extrinsic provided, prompts user to select the extrinsics fetched from runtime.
		if self.extrinsic.is_none() {
			self.update_extrinsics(cli, &mut registry).await?;
		}

		// Only prompt user to update parameters when `skip_menu` is not provided.
		if !self.skip_menu &&
			cli.confirm("Do you want to open the parameter menu?")
				.initial_value(false)
				.interact()?
		{
			self.ensure_pallet_registry(cli, &mut registry).await?;
			loop {
				let option = guide_user_to_select_menu_option(self, cli, &mut registry).await?;
				match option.update_arguments(self, &mut registry, cli).await {
					Ok(true) => break,
					Ok(false) => continue,
					Err(e) => cliclack::log::error(e)?,
				}
			}
		}

		// Prompt user to update output path of the benchmarking results.
		if self.output.is_none() {
			let input = cli
				.input("Provide the output file path for benchmark results (optional).")
				.required(false)
				.placeholder(".")
				.interact()?;
			self.output = if !input.is_empty() { Some(input.into()) } else { None };
		}

		cli.warning("NOTE: this may take some time...")?;
		cli.info("Benchmarking extrinsic weights of selected pallets...")?;
		let result = self.run();

		// Display the benchmarking command.
		cliclack::log::remark("\n")?;
		cli.success(self.display())?;
		if let Err(e) = result {
			return display_message(&e.to_string(), false, cli);
		}
		display_message("Benchmark completed successfully!", true, cli)?;
		Ok(())
	}

	fn run(&self) -> anyhow::Result<()> {
		generate_benchmarks(self.collect_arguments())
	}

	fn display(&self) -> String {
		let mut args = vec!["pop bench pallet".to_string()];
		let mut arguments = self.collect_arguments();
		if self.skip_menu {
			arguments.push("--skip".to_string());
		}
		args.extend(arguments);
		args.join(" ")
	}

	fn collect_arguments(&self) -> Vec<String> {
		let mut args = vec![];

		if self.list {
			args.push("--list".to_string());
		}

		if let Some(ref pallet) = self.pallet {
			args.push(format!(
				"--pallet={}",
				if is_selected_all(pallet) { String::new() } else { pallet.clone() }
			));
		}
		if let Some(ref extrinsic) = self.extrinsic {
			args.push(format!(
				"--extrinsic={}",
				if is_selected_all(extrinsic) { String::new() } else { extrinsic.clone() }
			));
		}
		if !self.exclude_pallets.is_empty() {
			args.push(format!("--exclude-pallets={}", self.exclude_pallets.join(",")));
		}

		args.push(format!("--steps={}", self.steps));

		if !self.lowest_range_values.is_empty() {
			args.push(format!(
				"--low={}",
				self.lowest_range_values
					.iter()
					.map(ToString::to_string)
					.collect::<Vec<_>>()
					.join(",")
			));
		}
		if !self.highest_range_values.is_empty() {
			args.push(format!(
				"--high={}",
				self.highest_range_values
					.iter()
					.map(ToString::to_string)
					.collect::<Vec<_>>()
					.join(",")
			));
		}

		args.extend([
			format!("--repeat={}", self.repeat),
			format!("--external-repeat={}", self.external_repeat),
			format!("--db-cache={}", self.database_cache_size),
			format!("--map-size={}", self.worst_case_map_values),
			format!("--additional-trie-layers={}", self.additional_trie_layers),
		]);

		if self.json_output {
			args.push("--json".to_string());
		}
		if let Some(ref json_file) = self.json_file {
			args.push(format!("--json-file={}", json_file.display()));
		}
		if self.no_median_slopes {
			args.push("--no-median-slopes".to_string());
		}
		if self.no_min_squares {
			args.push("--no-min-squares".to_string());
		}
		if self.no_storage_info {
			args.push("--no-storage-info".to_string());
		}
		if let Some(ref output) = self.output {
			let relative_output_path = get_relative_path(output.as_path());
			args.push(format!("--output={}", relative_output_path));
		}
		if let Some(ref template) = self.template {
			args.push(format!("--template={}", template.display()));
		}
		if let Some(ref output_analysis) = self.output_analysis {
			args.push(format!("--output-analysis={}", output_analysis));
		}
		if let Some(ref output_pov_analysis) = self.output_pov_analysis {
			args.push(format!("--output-pov-analysis={}", output_pov_analysis));
		}
		if let Some(ref heap_pages) = self.heap_pages {
			args.push(format!("--heap-pages={}", heap_pages));
		}
		if self.no_verify {
			args.push("--no-verify".to_string());
		}
		if self.extra {
			args.push("--extra".to_string());
		}
		if let Some(ref runtime) = self.runtime {
			args.push(format!("--runtime={}", runtime.display()));
		}
		if self.allow_missing_host_functions {
			args.push("--allow-missing-host-functions".to_string());
		}
		if let Some(ref genesis_builder) = self.genesis_builder {
			args.push(format!("--genesis-builder={}", genesis_builder));
			if genesis_builder == &GenesisBuilderPolicy::Runtime {
				args.push(format!("--genesis-builder-preset={}", self.genesis_builder_preset));
			}
		}
		args
	}

	// Guarantees that the registry is loaded before use. If not, it loads the registry.
	async fn ensure_pallet_registry(
		&self,
		cli: &mut impl cli::traits::Cli,
		registry: &mut PalletExtrinsicsRegistry,
	) -> anyhow::Result<()> {
		if registry.is_empty() {
			let runtime_path = self.runtime()?;
			let binary_path = check_omni_bencher_and_prompt(cli, true).await?;

			let spinner = spinner();
			spinner.start("Loading pallets and extrinsics from your runtime...");
			let loaded_registry =
				load_pallet_extrinsics(runtime_path, binary_path.as_path()).await?;
			spinner.clear();

			*registry = loaded_registry;
		}
		Ok(())
	}

	async fn update_pallets(
		&mut self,
		cli: &mut impl cli::traits::Cli,
		registry: &mut PalletExtrinsicsRegistry,
	) -> anyhow::Result<()> {
		self.ensure_pallet_registry(cli, registry).await?;
		let current_pallet = self.pallet.clone();
		let pallet = guide_user_to_select_pallet(registry, &self.exclude_pallets, cli)?;
		self.pallet = Some(pallet);

		if self.pallet != Some(ALL_SELECTED.to_string()) {
			// Reset the extrinsic to "*" when the pallet is changed.
			if self.pallet != current_pallet && self.extrinsic.is_some() {
				self.extrinsic = Some(ALL_SELECTED.to_string());
			}
		} else {
			self.extrinsic = Some(ALL_SELECTED.to_string())
		}
		Ok(())
	}

	async fn update_extrinsics(
		&mut self,
		cli: &mut impl cli::traits::Cli,
		registry: &mut PalletExtrinsicsRegistry,
	) -> anyhow::Result<()> {
		self.ensure_pallet_registry(cli, registry).await?;
		// Not allow selecting extrinsics when multiple pallets are selected.
		let pallet = self.pallet()?;
		self.extrinsic = Some(match pallet.clone() {
			s if s == *ALL_SELECTED => ALL_SELECTED.to_string(),
			_ => guide_user_to_select_extrinsics(pallet, registry, cli)?,
		});
		Ok(())
	}

	async fn update_excluded_pallets(
		&mut self,
		cli: &mut impl cli::traits::Cli,
		registry: &mut PalletExtrinsicsRegistry,
	) -> anyhow::Result<()> {
		self.ensure_pallet_registry(cli, registry).await?;
		let pallets = guide_user_to_exclude_pallets(registry, cli)?;
		self.exclude_pallets = pallets.into_iter().filter(|s| !s.is_empty()).collect();
		Ok(())
	}

	fn runtime(&self) -> anyhow::Result<&PathBuf> {
		self.runtime.as_ref().ok_or_else(|| anyhow::anyhow!("No runtime found"))
	}

	fn pallet(&self) -> anyhow::Result<&String> {
		self.pallet.as_ref().ok_or_else(|| anyhow::anyhow!("No pallet provided"))
	}

	fn extrinsic(&self) -> anyhow::Result<&String> {
		self.extrinsic.as_ref().ok_or_else(|| anyhow::anyhow!("No extrinsic provided"))
	}
}

#[derive(Clone, Copy, EnumIter, EnumMessageDerive, Eq, PartialEq)]
enum BenchmarkPalletMenuOption {
	/// Pallets to benchmark
	#[strum(message = "Pallets")]
	Pallets,
	/// Extrinsics inside the pallet to benchmark
	#[strum(message = "Extrinsics")]
	Extrinsics,
	/// Comma separated list of pallets that should be excluded from the benchmark
	#[strum(message = "Excluded pallets")]
	ExcludedPallets,
	/// Path to the runtime WASM binary
	#[strum(message = "Runtime path")]
	Runtime,
	/// How to construct the genesis state
	#[strum(message = "Genesis builder")]
	GenesisBuilder,
	/// The preset that we expect to find in the GenesisBuilder runtime API
	#[strum(message = "Genesis builder preset")]
	GenesisBuilderPreset,
	/// How many samples we should take across the variable components
	#[strum(message = "Steps")]
	Steps,
	/// How many repetitions of this benchmark should run from within the wasm
	#[strum(message = "Repeats")]
	Repeat,
	/// Indicates highest values for each of the component ranges
	#[strum(message = "High")]
	High,
	/// Indicates lowest values for each of the component ranges
	#[strum(message = "Low")]
	Low,
	/// The assumed default maximum size of any `StorageMap`
	#[strum(message = "Map size")]
	MapSize,
	/// Limit the memory (in MiB) the database cache can use
	#[strum(message = "Database cache size")]
	DatabaseCacheSize,
	/// Adjust the PoV estimation by adding additional trie layers to it
	#[strum(message = "Additional trie layer")]
	AdditionalTrieLayer,
	/// Don't print the median-slopes linear regression analysis
	#[strum(message = "No median slope")]
	NoMedianSlope,
	/// Don't print the min-squares linear regression analysis
	#[strum(message = "No min square")]
	NoMinSquare,
	/// If enabled, the storage info is not displayed in the output next to the analysis
	#[strum(message = "No storage info")]
	NoStorageInfo,
	#[strum(message = "> Save all parameter changes and continue")]
	SaveAndContinue,
}

impl BenchmarkPalletMenuOption {
	// Check if the menu option is disabled. If disabled, the menu option is not displayed in the
	// menu.
	fn is_disabled(
		self,
		cmd: &BenchmarkPallet,
		registry: &PalletExtrinsicsRegistry,
	) -> anyhow::Result<bool> {
		use BenchmarkPalletMenuOption::*;
		match self {
			// If there are multiple pallets provided, disable the extrinsics.
			Extrinsics => {
				let pallet = cmd.pallet()?;
				Ok(is_selected_all(pallet) || !registry.contains_key(pallet))
			},
			// Only allow excluding pallets if all pallets are selected.
			ExcludedPallets => Ok(!is_selected_all(cmd.pallet()?)),
			GenesisBuilder | GenesisBuilderPreset => {
				let presets = get_preset_names(cmd.runtime()?)?;
				// If there are no presets available, disable the preset builder options.
				if presets.is_empty() {
					return Ok(true);
				}
				if self == GenesisBuilderPreset {
					return Ok(cmd.genesis_builder == Some(GenesisBuilderPolicy::None));
				}
				Ok(false)
			},
			_ => Ok(false),
		}
	}

	// Reads the command argument based on the selected menu option.
	//
	// This method retrieves the appropriate value from `PalletCmd` depending on
	// the `BenchmarkPalletMenuOption` variant. It formats the value as a string
	// for display or further processing.
	fn read_command(self, cmd: &BenchmarkPallet) -> anyhow::Result<String> {
		use BenchmarkPalletMenuOption::*;
		Ok(match self {
			Pallets => self.get_joined_string(cmd.pallet()?),
			Extrinsics => self.get_joined_string(cmd.extrinsic()?),
			ExcludedPallets =>
				if cmd.exclude_pallets.is_empty() {
					"None".to_string()
				} else {
					cmd.exclude_pallets.join(",")
				},
			Runtime => get_relative_path(cmd.runtime()?),
			GenesisBuilder => cmd.genesis_builder.unwrap_or(GenesisBuilderPolicy::None).to_string(),
			GenesisBuilderPreset => cmd.genesis_builder_preset.clone(),
			Steps => cmd.steps.to_string(),
			Repeat => cmd.repeat.to_string(),
			High => self.get_range_values(&cmd.highest_range_values),
			Low => self.get_range_values(&cmd.lowest_range_values),
			MapSize => cmd.worst_case_map_values.to_string(),
			DatabaseCacheSize => cmd.database_cache_size.to_string(),
			AdditionalTrieLayer => cmd.additional_trie_layers.to_string(),
			NoMedianSlope => cmd.no_median_slopes.to_string(),
			NoMinSquare => cmd.no_min_squares.to_string(),
			NoStorageInfo => cmd.no_storage_info.to_string(),
			SaveAndContinue => String::default(),
		})
	}

	// Implementation to update the command argument when the menu option is selected.
	async fn update_arguments(
		self,
		cmd: &mut BenchmarkPallet,
		registry: &mut PalletExtrinsicsRegistry,
		cli: &mut impl cli::traits::Cli,
	) -> anyhow::Result<bool> {
		use BenchmarkPalletMenuOption::*;
		match self {
			Pallets => cmd.update_pallets(cli, registry).await?,
			Extrinsics => cmd.update_extrinsics(cli, registry).await?,
			ExcludedPallets => cmd.update_excluded_pallets(cli, registry).await?,
			Runtime =>
				cmd.runtime = Some(ensure_runtime_binary_exists(
					cli,
					&get_current_directory(),
					&Profile::Release,
				)?),
			GenesisBuilder =>
				cmd.genesis_builder =
					Some(guide_user_to_select_genesis_policy(cli, &cmd.genesis_builder)?),
			GenesisBuilderPreset => {
				cmd.genesis_builder_preset = guide_user_to_select_genesis_preset(
					cli,
					cmd.runtime()?,
					&cmd.genesis_builder_preset,
				)?;
			},
			Steps => cmd.steps = self.input_parameter(cmd, cli, true)?.parse()?,
			Repeat => cmd.repeat = self.input_parameter(cmd, cli, true)?.parse()?,
			High => cmd.highest_range_values = self.input_range_values(cmd, cli, true)?,
			Low => cmd.lowest_range_values = self.input_range_values(cmd, cli, true)?,
			MapSize => cmd.worst_case_map_values = self.input_parameter(cmd, cli, true)?.parse()?,
			DatabaseCacheSize =>
				cmd.database_cache_size = self.input_parameter(cmd, cli, true)?.parse()?,
			AdditionalTrieLayer =>
				cmd.additional_trie_layers = self.input_parameter(cmd, cli, true)?.parse()?,
			NoMedianSlope => cmd.no_median_slopes = self.confirm(cmd, cli)?,
			NoMinSquare => cmd.no_min_squares = self.confirm(cmd, cli)?,
			NoStorageInfo => cmd.no_storage_info = self.confirm(cmd, cli)?,
			SaveAndContinue => return Ok(true),
		};
		Ok(false)
	}

	fn input_parameter(
		self,
		cmd: &BenchmarkPallet,
		cli: &mut impl cli::traits::Cli,
		is_required: bool,
	) -> anyhow::Result<String> {
		let default_value = self.read_command(cmd)?;
		let prompt_message = format!(
			r#"Provide value to the parameter "{}""#,
			self.get_message().unwrap_or_default(),
		);
		cli.input(prompt_message)
			.required(is_required)
			.placeholder(&default_value)
			.default_input(&default_value)
			.interact()
			.map(|v| v.trim().to_string())
			.map_err(anyhow::Error::from)
	}

	fn input_range_values(
		self,
		cmd: &BenchmarkPallet,
		cli: &mut impl cli::traits::Cli,
		is_required: bool,
	) -> anyhow::Result<Vec<u32>> {
		let values = self.input_array(
			cmd,
			&format!(
				r#"Provide range values to the parameter "{}" (numbers separated by commas)"#,
				self.get_message().unwrap_or_default()
			),
			cli,
			is_required,
		)?;

		let mut parsed_inputs = vec![];
		for value in values {
			parsed_inputs.push(value.parse()?);
		}
		Ok(parsed_inputs)
	}

	fn input_array(
		self,
		cmd: &BenchmarkPallet,
		label: &str,
		cli: &mut impl cli::traits::Cli,
		is_required: bool,
	) -> anyhow::Result<Vec<String>> {
		let default_value = self.read_command(cmd)?;
		let input = cli
			.input(label)
			.required(is_required)
			.placeholder(&default_value)
			.default_input(&default_value)
			.interact()
			.map(|v| v.trim().to_string())
			.map_err(anyhow::Error::from)?;
		Ok(input.split(",").map(String::from).collect())
	}

	fn confirm(
		self,
		cmd: &BenchmarkPallet,
		cli: &mut impl cli::traits::Cli,
	) -> anyhow::Result<bool> {
		let default_value = self.read_command(cmd)?;
		let parsed_default_value = default_value.trim().parse().unwrap();
		cli.confirm(format!(
			r#"Do you want to enable "{}"?"#,
			self.get_message().unwrap_or_default()
		))
		.initial_value(parsed_default_value)
		.interact()
		.map_err(anyhow::Error::from)
	}

	fn get_range_values<T: ToString>(self, range_values: &[T]) -> String {
		if range_values.is_empty() {
			return "None".to_string();
		}
		range_values.iter().map(ToString::to_string).collect::<Vec<_>>().join(",")
	}

	fn get_joined_string(self, s: &String) -> String {
		if is_selected_all(s) {
			return "All selected".to_string()
		}
		s.clone()
	}
}

fn guide_user_to_select_pallet(
	registry: &PalletExtrinsicsRegistry,
	excluded_pallets: &[String],
	cli: &mut impl cli::traits::Cli,
) -> anyhow::Result<String> {
	let pallets = pallets(registry, excluded_pallets);
	if pallets.is_empty() {
		return Err(anyhow::anyhow!("No pallets found for the runtime"));
	}

	if cli
		.confirm("Would you like to benchmark all pallets?")
		.initial_value(true)
		.interact()?
	{
		return Ok(ALL_SELECTED.to_string());
	}

	let mut prompt = cli.select(r#"🔎 Search for a pallet to benchmark"#).filter_mode();
	for pallet in pallets {
		prompt = prompt.item(pallet.clone(), &pallet, "");
	}
	Ok(prompt.interact()?.to_string())
}

fn guide_user_to_exclude_pallets(
	registry: &PalletExtrinsicsRegistry,
	cli: &mut impl cli::traits::Cli,
) -> anyhow::Result<Vec<String>> {
	let mut prompt = cli
		.multiselect(r#"🔎 Search for pallets to exclude (Press ENTER to skip)"#)
		.required(false);
	for pallet in pallets(registry, &[]) {
		prompt = prompt.item(pallet.clone(), &pallet, "");
	}
	Ok(prompt.interact()?)
}

fn guide_user_to_select_extrinsics(
	pallet: &String,
	registry: &PalletExtrinsicsRegistry,
	cli: &mut impl cli::traits::Cli,
) -> anyhow::Result<String> {
	let extrinsics = extrinsics(registry, pallet);
	if extrinsics.is_empty() {
		return Err(anyhow::anyhow!("No extrinsics found for the pallet"));
	}

	if cli
		.confirm(format!(r#"Would you like to benchmark all extrinsics of "{}"?"#, pallet))
		.initial_value(true)
		.interact()?
	{
		return Ok(ALL_SELECTED.to_string());
	}

	let mut prompt = cli
		.multiselect(r#"🔎 Search for extrinsics to benchmark (select with space)"#)
		.filter_mode()
		.required(true);
	for extrinsic in extrinsics {
		prompt = prompt.item(extrinsic.clone(), &extrinsic, "");
	}
	Ok(prompt.interact()?.join(","))
}

async fn guide_user_to_select_menu_option(
	cmd: &mut BenchmarkPallet,
	cli: &mut impl cli::traits::Cli,
	registry: &mut PalletExtrinsicsRegistry,
) -> anyhow::Result<BenchmarkPalletMenuOption> {
	let spinner = spinner();
	let mut prompt = cli.select("Select the parameter to update:");

	let mut index = 0;
	spinner.start("Loading parameter menu...");
	for param in BenchmarkPalletMenuOption::iter() {
		if param.is_disabled(cmd, registry)? {
			continue;
		}
		let label = param.get_message().unwrap_or_default();
		let hint = param.get_documentation().unwrap_or_default();
		let formatted_label = match param {
			BenchmarkPalletMenuOption::SaveAndContinue => label,
			_ => &format!("({index}) - {label}: {}", param.read_command(cmd)?),
		};
		prompt = prompt.item(param, formatted_label, hint);
		index += 1;
	}
	spinner.clear();
	Ok(prompt.interact()?)
}

fn is_selected_all(s: &String) -> bool {
	s == &ALL_SELECTED.to_string() || s.is_empty()
}

fn pallets(registry: &PalletExtrinsicsRegistry, excluded_pallets: &[String]) -> Vec<String> {
	registry
		.keys()
		.filter(|s| !excluded_pallets.contains(&s.to_string()))
		.map(String::from)
		.collect()
}

fn extrinsics(registry: &PalletExtrinsicsRegistry, pallet: &str) -> Vec<String> {
	registry.get(pallet).cloned().unwrap_or_default()
}

// Add a more relaxed parsing for pallet names by allowing pallet directory names with `-` to be
// used like crate names with `_`
fn parse_pallet_name(pallet: &str) -> std::result::Result<String, String> {
	Ok(pallet.replace("-", "_"))
}

// Get relative path. Returns absolute path if the path is not relative.
fn get_relative_path(path: &Path) -> String {
	let cwd = current_dir().unwrap_or(PathBuf::from("./"));
	let path = get_relative_or_absolute_path(cwd.as_path(), path);
	path.as_path().to_str().expect("No path provided").to_string()
}

fn get_current_directory() -> PathBuf {
	current_dir().unwrap_or(PathBuf::from("./"))
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::{
		cli::MockCli,
		common::bench::{get_mock_runtime, source_omni_bencher_binary},
	};
	use anyhow::Ok;
	use std::env::current_dir;

	#[tokio::test]
	async fn benchmark_pallet_works() -> anyhow::Result<()> {
		let mut cli = MockCli::new();
		let temp_dir = tempfile::tempdir()?;

		let cwd = current_dir().unwrap_or(PathBuf::from("./"));
		let runtime_path = get_mock_runtime(true);
		let output_path = temp_dir.path().join("weights.rs");

		cli = expect_pallet_benchmarking_intro(cli)
			.expect_warning(format!(
				"No runtime folder found at {}. Please input the runtime path manually.",
				cwd.display()
			))
			.expect_input(
				"Please provide the path to the runtime.",
				runtime_path.to_str().unwrap().to_string(),
			)
			.expect_confirm("Do you want to open the parameter menu?", false)
			.expect_warning("NOTE: this may take some time...")
			.expect_info("Benchmarking extrinsic weights of selected pallets...")
			.expect_input(
				"Provide the output file path for benchmark results (optional).",
				output_path.to_str().unwrap().to_string(),
			);

		let mut cmd = BenchmarkPallet {
			skip_confirm: false,
			genesis_builder: Some(GenesisBuilderPolicy::None),
			pallet: Some("pallet_timestamp".to_string()),
			extrinsic: Some(ALL_SELECTED.to_string()),
			..Default::default()
		};
		cmd.execute(&mut cli).await?;
		assert!(output_path.exists());

		// Verify the printed command.
		cli = cli
			.expect_success(cmd.display())
			.expect_outro("Benchmark completed successfully!");
		cmd.execute(&mut cli).await?;
		cli.verify()
	}

	#[tokio::test]
	async fn list_pallets_works() -> anyhow::Result<()> {
		for with_benchmark_feature in [true] {
			let mut cli = MockCli::new()
			.expect_intro("Listing available pallets and extrinsics")
			.expect_warning(
				"NOTE: the `pop bench pallet` is not yet battle tested - double check the results.",
			)
			.expect_outro("All pallets and extrinsics listed!");
			BenchmarkPallet {
				list: true,
				runtime: Some(get_mock_runtime(with_benchmark_feature)),
				..Default::default()
			}
			.execute(&mut cli)
			.await?;
			cli.verify()?;
		}
		Ok(())
	}

	#[tokio::test]
	async fn benchmark_pallet_without_runtime_benchmarks_feature_fails() -> anyhow::Result<()> {
		let mut cli = MockCli::new();
		cli = expect_pallet_benchmarking_intro(cli);
		cli = cli.expect_outro_cancel(
	        "Failed to run benchmarking: Invalid input: Could not call runtime API to Did not find the benchmarking metadata. \
	        This could mean that you either did not build the node correctly with the `--features runtime-benchmarks` flag, \
			or the chain spec that you are using was not created by a node that was compiled with the flag: \
			Other: Exported method Benchmark_benchmark_metadata is not found"
		);

		BenchmarkPallet {
			runtime: Some(get_mock_runtime(false)),
			pallet: Some("pallet_timestamp".to_string()),
			extrinsic: Some(ALL_SELECTED.to_string()),
			skip_menu: true,
			genesis_builder: Some(GenesisBuilderPolicy::None),
			..Default::default()
		}
		.execute(&mut cli)
		.await?;
		cli.verify()
	}

	#[tokio::test]
	async fn benchmark_pallet_fails_with_error() -> anyhow::Result<()> {
		let mut cli = MockCli::new();
		cli = expect_pallet_benchmarking_intro(cli);
		cli = cli.expect_outro_cancel("Failed to run benchmarking: Invalid input: No benchmarks found which match your input.");

		BenchmarkPallet {
			runtime: Some(get_mock_runtime(true)),
			pallet: Some("unknown_pallet".to_string()),
			extrinsic: Some(ALL_SELECTED.to_string()),
			skip_menu: true,
			genesis_builder: Some(GenesisBuilderPolicy::None),
			..Default::default()
		}
		.execute(&mut cli)
		.await?;
		cli.verify()
	}

	#[tokio::test]
	async fn guide_user_to_select_pallet_works() -> anyhow::Result<()> {
		let runtime_path = get_mock_runtime(true);
		let binary_path =
			source_omni_bencher_binary(&mut MockCli::new(), &crate::cache()?, true).await?;
		let registry = load_pallet_extrinsics(&runtime_path, binary_path.as_path()).await?;
		let pallet_items: Vec<(String, String)> = pallets(&registry, &[])
			.into_iter()
			.map(|pallet| (pallet, Default::default()))
			.collect();
		let prompt = "Would you like to benchmark all pallets?";

		// Select all pallets.
		let mut cli = MockCli::new().expect_confirm(prompt, true);
		assert_eq!(
			guide_user_to_select_pallet(&registry, &[], &mut cli)?,
			ALL_SELECTED.to_string()
		);
		cli.verify()?;

		// Not exclude pallets.
		cli = MockCli::new().expect_confirm(prompt, false).expect_select(
			r#"🔎 Search for a pallet to benchmark"#,
			None,
			true,
			Some(pallet_items.clone()),
			0,
			Some(true),
		);
		guide_user_to_select_pallet(&registry, &[], &mut cli)?;
		cli.verify()?;

		// Exclude pallets
		cli = MockCli::new().expect_confirm(prompt, false).expect_select(
			r#"🔎 Search for a pallet to benchmark"#,
			None,
			true,
			Some(pallet_items.into_iter().filter(|(p, _)| p != "pallet_timestamp").collect()),
			0,
			Some(true),
		);
		guide_user_to_select_pallet(&registry, &["pallet_timestamp".to_string()], &mut cli)?;
		cli.verify()
	}

	#[tokio::test]
	async fn guide_user_to_exclude_pallets_works() -> anyhow::Result<()> {
		let mut cli = MockCli::new();
		let runtime_path = get_mock_runtime(true);
		let binary_path = source_omni_bencher_binary(&mut cli, &crate::cache()?, true).await?;
		let registry = load_pallet_extrinsics(&runtime_path, binary_path.as_path()).await?;
		let pallet_items = pallets(&registry, &[])
			.into_iter()
			.map(|pallet| (pallet, Default::default()))
			.collect();
		cli = MockCli::new().expect_multiselect::<String>(
			r#"🔎 Search for pallets to exclude (Press ENTER to skip)"#,
			Some(false),
			true,
			Some(pallet_items),
			Some(true),
		);
		guide_user_to_exclude_pallets(&registry, &mut cli)?;
		cli.verify()
	}

	#[tokio::test]
	async fn guide_user_to_select_extrinsics_works() -> anyhow::Result<()> {
		let mut cli = MockCli::new();
		let runtime_path = get_mock_runtime(true);
		let binary_path = source_omni_bencher_binary(&mut cli, &crate::cache()?, true).await?;
		let registry = load_pallet_extrinsics(&runtime_path, binary_path.as_path()).await?;
		let extrinsic_items = extrinsics(&registry, "pallet_timestamp")
			.into_iter()
			.map(|pallet| (pallet, Default::default()))
			.collect();

		let mut cli = MockCli::new().expect_confirm(
			r#"Would you like to benchmark all extrinsics of "pallet_timestamp"?"#,
			true,
		);
		assert_eq!(
			guide_user_to_select_extrinsics(&"pallet_timestamp".to_string(), &registry, &mut cli)?,
			ALL_SELECTED.to_string()
		);

		cli = MockCli::new()
			.expect_confirm(
				r#"Would you like to benchmark all extrinsics of "pallet_timestamp"?"#,
				false,
			)
			.expect_multiselect::<String>(
				r#"🔎 Search for extrinsics to benchmark (select with space)"#,
				Some(true),
				true,
				Some(extrinsic_items),
				Some(true),
			);
		guide_user_to_select_extrinsics(&"pallet_timestamp".to_string(), &registry, &mut cli)?;
		cli.verify()
	}

	#[tokio::test]
	async fn guide_user_to_select_menu_option_works() -> anyhow::Result<()> {
		let runtime_path = get_mock_runtime(true);
		let binary_path =
			source_omni_bencher_binary(&mut MockCli::new(), &crate::cache()?, true).await?;
		let mut registry = load_pallet_extrinsics(&runtime_path, binary_path.as_path()).await?;

		let mut cmd = BenchmarkPallet {
			skip_confirm: false,
			runtime: Some(get_mock_runtime(true)),
			pallet: Some(ALL_SELECTED.to_string()),
			..Default::default()
		};

		let mut cli = expect_parameter_menu(MockCli::new(), &cmd, &registry, 0)?;
		guide_user_to_select_menu_option(&mut cmd, &mut cli, &mut registry).await?;
		cli.verify()
	}

	#[tokio::test]
	async fn menu_option_is_disabled_works() -> anyhow::Result<()> {
		use BenchmarkPalletMenuOption::*;
		let mut cli = MockCli::new();
		let runtime_path = get_mock_runtime(true);
		let binary_path = source_omni_bencher_binary(&mut cli, &crate::cache()?, true).await?;
		let registry = load_pallet_extrinsics(&runtime_path, binary_path.as_path()).await?;

		let cmd = BenchmarkPallet {
			runtime: Some(get_mock_runtime(false)),
			pallet: Some(ALL_SELECTED.to_string()),
			extrinsic: Some(ALL_SELECTED.to_string()),
			genesis_builder: Some(GenesisBuilderPolicy::None),
			..Default::default()
		};
		assert!(!GenesisBuilder.is_disabled(&cmd, &registry)?);
		assert!(GenesisBuilderPreset.is_disabled(&cmd, &registry)?);
		assert!(Extrinsics.is_disabled(&cmd, &registry)?);
		assert!(ExcludedPallets.is_disabled(
			&mut BenchmarkPallet {
				pallet: Some("pallet_timestamp".to_string()),
				..Default::default()
			},
			&registry
		)?);
		Ok(())
	}

	#[test]
	fn menu_option_read_command_works() -> anyhow::Result<()> {
		use BenchmarkPalletMenuOption::*;
		let cmd = BenchmarkPallet {
			runtime: Some(get_mock_runtime(false)),
			pallet: Some(ALL_SELECTED.to_string()),
			extrinsic: Some(ALL_SELECTED.to_string()),
			genesis_builder: Some(GenesisBuilderPolicy::Runtime),
			..Default::default()
		};
		[
			(Pallets, "All selected"),
			(Extrinsics, "All selected"),
			(ExcludedPallets, "None"),
			(Runtime, get_mock_runtime(false).to_str().unwrap()),
			(GenesisBuilder, &GenesisBuilderPolicy::Runtime.to_string()),
			(GenesisBuilderPreset, "development"),
			(Steps, "50"),
			(Repeat, "20"),
			(High, "None"),
			(Low, "None"),
			(MapSize, "1000000"),
			(DatabaseCacheSize, "1024"),
			(AdditionalTrieLayer, "2"),
			(NoMedianSlope, "false"),
			(NoMinSquare, "false"),
			(NoStorageInfo, "false"),
		]
		.into_iter()
		.for_each(|(option, value)| {
			assert_eq!(option.read_command(&cmd).unwrap(), value.to_string());
		});
		Ok(())
	}

	#[test]
	fn menu_option_input_parameter_works() -> anyhow::Result<()> {
		use BenchmarkPalletMenuOption::*;
		let mut cli = MockCli::new();
		let cmd = BenchmarkPallet::default();
		let options = [
			(Steps, "100"),
			(Repeat, "40"),
			(High, "10,20"),
			(Low, "10,20"),
			(MapSize, "50000"),
			(DatabaseCacheSize, "2048"),
			(AdditionalTrieLayer, "4"),
		];
		for (option, value) in options.to_vec().into_iter() {
			cli = cli.expect_input(
				format!(
					r#"Provide value to the parameter "{}""#,
					option.get_message().unwrap_or_default()
				),
				value.to_string(),
			);
		}
		for (option, _) in options.to_vec() {
			option.input_parameter(&cmd, &mut cli, true)?;
		}
		cli.verify()
	}

	#[test]
	fn menu_option_input_range_values_works() -> anyhow::Result<()> {
		use BenchmarkPalletMenuOption::*;
		let mut cli = MockCli::new();
		let cmd = BenchmarkPallet::default();
		let options = [High, Low];
		for option in options.into_iter() {
			cli = cli.expect_input(
				&format!(
					r#"Provide range values to the parameter "{}" (numbers separated by commas)"#,
					option.get_message().unwrap_or_default()
				),
				"10,20,30".to_string(),
			);
		}
		for option in options.into_iter() {
			option.input_range_values(&cmd, &mut cli, true)?;
		}
		cli.verify()
	}

	#[test]
	fn menu_option_confirm_works() -> anyhow::Result<()> {
		use BenchmarkPalletMenuOption::*;
		let mut cli = MockCli::new();
		let cmd = BenchmarkPallet::default();
		let options = [(NoStorageInfo, false), (NoMinSquare, false), (NoMedianSlope, false)];
		for (option, value) in options.into_iter() {
			cli = cli.expect_confirm(
				format!(r#"Do you want to enable "{}"?"#, option.get_message().unwrap_or_default()),
				value,
			);
		}
		for (option, _) in options.into_iter() {
			option.confirm(&cmd, &mut cli)?;
		}
		cli.verify()
	}

	#[tokio::test]
	async fn ensure_pallet_registry_works() -> anyhow::Result<()> {
		let mut cli = MockCli::new();
		let runtime_path = get_mock_runtime(true);
		let cmd = BenchmarkPallet { runtime: Some(runtime_path), ..Default::default() };
		let mut registry = PalletExtrinsicsRegistry::default();

		// Load pallet registry if the cached registry is empty.
		cmd.ensure_pallet_registry(&mut cli, &mut registry).await?;
		let mut pallet_names: Vec<String> = registry.keys().map(String::from).collect();
		pallet_names.sort_by(|a, b| a.cmp(b));
		assert_eq!(
			pallet_names,
			vec![
				"cumulus_pallet_parachain_system".to_string(),
				"cumulus_pallet_xcmp_queue".to_string(),
				"frame_system".to_string(),
				"pallet_balances".to_string(),
				"pallet_collator_selection".to_string(),
				"pallet_message_queue".to_string(),
				"pallet_session".to_string(),
				"pallet_sudo".to_string(),
				"pallet_timestamp".to_string()
			]
		);

		// If the pallet registry already exists, skip loading it.
		let mock_registry = PalletExtrinsicsRegistry::from([
			("pallet_timestamp".to_string(), vec!["on_finalize".to_string(), "set".to_string()]),
			("frame_system".to_string(), vec!["set_code".to_string(), "remark".to_string()]),
		]);
		registry = mock_registry.clone();
		cmd.ensure_pallet_registry(&mut cli, &mut registry).await?;
		assert_eq!(registry, mock_registry);

		Ok(())
	}

	#[test]
	fn get_runtime_works() -> anyhow::Result<()> {
		assert_eq!(
			BenchmarkPallet { runtime: Some(get_mock_runtime(false)), ..Default::default() }
				.runtime()?,
			&get_mock_runtime(false)
		);
		assert!(matches!(BenchmarkPallet::default().runtime(), Err(message)
			if message.to_string().contains("No runtime found")
		));
		Ok(())
	}

	#[test]
	fn get_pallet_works() -> anyhow::Result<()> {
		assert_eq!(
			BenchmarkPallet { pallet: Some("pallet_timestamp".to_string()), ..Default::default() }
				.pallet()?,
			&"pallet_timestamp".to_string()
		);
		assert!(matches!(BenchmarkPallet::default().pallet(), Err(message)
			if message.to_string().contains("No pallet provided")
		));
		Ok(())
	}

	#[test]
	fn get_extrinsic_works() -> anyhow::Result<()> {
		assert_eq!(
			BenchmarkPallet { extrinsic: Some("set".to_string()), ..Default::default() }
				.extrinsic()?,
			&"set".to_string()
		);
		assert!(matches!(BenchmarkPallet::default().extrinsic(), Err(message)
			if message.to_string().contains("No extrinsic provided")
		));
		Ok(())
	}

	fn expect_pallet_benchmarking_intro(cli: MockCli) -> MockCli {
		cli.expect_intro("Benchmarking your pallets").expect_warning(
			"NOTE: the `pop bench pallet` is not yet battle tested - double check the results.",
		)
	}

	fn expect_parameter_menu(
		cli: MockCli,
		cmd: &BenchmarkPallet,
		registry: &PalletExtrinsicsRegistry,
		item: usize,
	) -> anyhow::Result<MockCli> {
		let mut items: Vec<(String, String)> = vec![];
		let mut index = 0;
		for param in BenchmarkPalletMenuOption::iter() {
			if param.is_disabled(cmd, registry)? {
				continue;
			}
			let label = param.get_message().unwrap_or_default();
			let hint = param.get_documentation().unwrap_or_default();
			let formatted_label = match param {
				BenchmarkPalletMenuOption::SaveAndContinue => label,
				_ => &format!("({index}) - {label}: {}", param.read_command(cmd)?),
			};
			items.push((formatted_label.to_string(), hint.to_string()));
			index += 1;
		}
		Ok(cli.expect_select(
			"Select the parameter to update:",
			Some(true),
			true,
			Some(items),
			item,
			Some(false),
		))
	}
}
