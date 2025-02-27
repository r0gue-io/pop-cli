// SPDX-License-Identifier: GPL-3.0

use super::display_message;
use crate::{
	cli::{
		self,
		traits::{Confirm, Input, MultiSelect, Select},
	},
	common::bench::check_omni_bencher_and_prompt,
};
use clap::Args;
use cliclack::spinner;
use pop_common::{manifest::from_path, Profile};
use pop_parachains::{
	build_project, generate_benchmarks, get_preset_names, get_relative_runtime_path,
	get_runtime_path, load_pallet_extrinsics, runtime_binary_path, search_for_extrinsics,
	search_for_pallets, GenesisBuilderPolicy, PalletExtrinsicsRegistry, GENESIS_BUILDER_DEV_PRESET,
};
use std::{
	collections::HashMap,
	env::current_dir,
	ffi::OsStr,
	fs,
	path::{Path, PathBuf},
};
use strum::{EnumMessage, IntoEnumIterator};
use strum_macros::{EnumIter, EnumMessage as EnumMessageDerive};

const ALL_SELECTED: &str = "*";
const MAX_EXTRINSIC_LIMIT: usize = 15;
const MAX_PALLET_LIMIT: usize = 20;

#[derive(Args, Default)]
pub(crate) struct BenchmarkPallet {
	/// Select a FRAME Pallet to benchmark, or `*` for all (in which case `extrinsic` must be `*`).
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

impl BenchmarkPallet {
	pub async fn execute(&mut self, cli: &mut impl cli::traits::Cli) -> anyhow::Result<()> {
		if self.list || self.json_output {
			if let Err(e) = self.run() {
				return display_message(&e.to_string(), false, cli);
			}
		}
		// If `all` is provided, we override the value of `pallet` and `extrinsic` to select all.
		if self.all {
			self.pallet = Some(ALL_SELECTED.to_string());
			self.extrinsic = Some(ALL_SELECTED.to_string());
			self.all = false;
		}

		let mut registry: PalletExtrinsicsRegistry = HashMap::default();
		cli.intro("Benchmarking your pallets")?;
		cli.warning(
			"NOTE: the `pop bench pallet` is not yet battle tested - double check the results.",
		)?;

		// No runtime path provided, auto-detect the runtime WASM binary. If not found, build
		// the runtime.
		if self.runtime.is_none() {
			match ensure_runtime_binary_exists(cli, &Profile::Release) {
				Ok(runtime_binary_path) => self.runtime = Some(runtime_binary_path),
				Err(e) => {
					return display_message(&e.to_string(), false, cli);
				},
			}
		}
		// No genesis builder, prompts user to select the genesis builder policy.
		if self.genesis_builder.is_none() {
			if let Err(e) = self.update_genesis_builder(cli) {
				return display_message(&e.to_string(), false, cli);
			};
		}
		// No pallet provided, prompts user to select the pallets fetched from runtime.
		if self.pallet.is_none() {
			self.update_pallets(cli, &mut registry).await?;
		}
		// No extrinsic provided, prompts user to select the extrinsics fetched from runtime.
		if self.extrinsic.is_none() {
			self.update_extrinsics(cli, &mut registry).await?;
		}

		// Only prompt user to update parameters when `skip_menu` is not provided.
		if !self.skip_menu {
			self.fetch_pallet_registry(cli, &mut registry).await?;
			loop {
				let option = guide_user_to_select_menu_option(self, cli, &mut registry).await?;
				match option.update_arguments(self, &mut registry, cli).await {
					Ok(true) => break,
					Ok(false) => continue,
					Err(e) => cli.info(e)?,
				}
			}
		}

		cli.warning("NOTE: this may take some time...")?;
		cli.info("Benchmarking extrinsic weights of selected pallets...")?;
		let result = self.run();

		// Display the benchmarking command.
		let mut message = self.display();
		if self.skip_menu {
			message.push_str(" --skip");
		}
		cli.info(message)?;
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
		let arguments = self.collect_arguments();
		args.extend(arguments);
		args.join(" ")
	}

	fn collect_arguments(&self) -> Vec<String> {
		let mut args = vec![];

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
			args.push(format!("--output={}", output.display()));
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
			args.push(format!("--genesis-builder={}", genesis_builder.to_string()));
			if genesis_builder == &GenesisBuilderPolicy::Runtime {
				args.push(format!("--genesis-builder-preset={}", self.genesis_builder_preset));
			}
		}
		args
	}

	async fn fetch_pallet_registry(
		&self,
		cli: &mut impl cli::traits::Cli,
		registry: &mut PalletExtrinsicsRegistry,
	) -> anyhow::Result<()> {
		if registry.is_empty() {
			let runtime_path = self.runtime()?;
			let binary_path = check_omni_bencher_and_prompt(cli, &crate::cache()?, true).await?;

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
		self.fetch_pallet_registry(cli, registry).await?;
		self.pallet = Some(guide_user_to_select_pallets(registry, &self.exclude_pallets, cli)?);
		Ok(())
	}

	async fn update_extrinsics(
		&mut self,
		cli: &mut impl cli::traits::Cli,
		registry: &mut PalletExtrinsicsRegistry,
	) -> anyhow::Result<()> {
		self.fetch_pallet_registry(cli, registry).await?;
		// Not allow selecting extrinsics when multiple pallets are selected.
		let pallet_count = self.pallet()?.matches(",").count();
		self.extrinsic = Some(match pallet_count {
			0 => guide_user_to_select_extrinsics(self.pallet()?, registry, cli)?,
			_ => ALL_SELECTED.to_string(),
		});
		Ok(())
	}

	async fn update_excluded_pallets(
		&mut self,
		cli: &mut impl cli::traits::Cli,
		registry: &mut PalletExtrinsicsRegistry,
	) -> anyhow::Result<()> {
		self.fetch_pallet_registry(cli, registry).await?;
		let pallets = guide_user_to_exclude_pallets(registry, cli)?;
		self.exclude_pallets =
			pallets.split(',').filter(|s| !s.is_empty()).map(|s| s.to_string()).collect();
		Ok(())
	}

	fn update_genesis_preset(&mut self, cli: &mut impl cli::traits::Cli) -> anyhow::Result<()> {
		self.genesis_builder_preset = guide_user_to_select_genesis_preset(
			cli,
			self.runtime()?,
			&self.genesis_builder_preset,
		)?;
		Ok(())
	}

	fn update_genesis_builder(&mut self, cli: &mut impl cli::traits::Cli) -> anyhow::Result<()> {
		let runtime_path = self.runtime()?;
		let preset_names = get_preset_names(runtime_path)?;
		// Determine policy based on preset availability.
		let policy = if preset_names.is_empty() {
			GenesisBuilderPolicy::Runtime
		} else {
			guide_user_to_select_genesis_policy(cli)?
		};
		// If the policy requires a preset, prompt the user to select one.
		if policy == GenesisBuilderPolicy::Runtime {
			let preset = guide_user_to_select_genesis_preset(
				cli,
				runtime_path,
				&self.genesis_builder_preset,
			)?;
			self.genesis_builder_preset = preset;
		}
		self.genesis_builder = Some(policy);
		Ok(())
	}

	fn runtime(&self) -> anyhow::Result<&PathBuf> {
		match self.runtime.as_ref() {
			Some(runtime) => Ok(runtime),
			None => Err(anyhow::anyhow!("No runtime found")),
		}
	}

	fn pallet(&self) -> anyhow::Result<&String> {
		match self.pallet.as_ref() {
			Some(pallet) => Ok(pallet),
			None => Err(anyhow::anyhow!("No pallet provided")),
		}
	}

	fn extrinsic(&self) -> anyhow::Result<&String> {
		match self.extrinsic.as_ref() {
			Some(extinsic) => Ok(extinsic),
			None => Err(anyhow::anyhow!("No extrinsic provided")),
		}
	}
}

#[derive(Clone, Copy, EnumIter, EnumMessageDerive, Eq, PartialEq)]
enum BenchmarkPalletMenuOption {
	/// FRAME Pallets to benchmark
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
	///  If enabled, the storage info is not displayed in the output next to the analysis
	#[strum(message = "No storage info")]
	NoStorageInfo,
	#[strum(message = "> Save all parameter changes and continue")]
	SaveAndContinue,
}

impl BenchmarkPalletMenuOption {
	// Check if the menu option is disabled. If disabled, the menu option is not displayed in the
	// menu.
	async fn is_disabled(
		self,
		cmd: &BenchmarkPallet,
		registry: &PalletExtrinsicsRegistry,
	) -> anyhow::Result<bool> {
		use BenchmarkPalletMenuOption::*;
		match self {
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
			// If there are multiple pallets provided, disable the extrinsics.
			Extrinsics => {
				let pallet = cmd.pallet()?;
				Ok(is_selected_all(&pallet) || !registry.contains_key(pallet))
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
			Runtime => get_relative_runtime_path(cmd.runtime()?),
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
			Runtime => cmd.runtime = Some(ensure_runtime_binary_exists(cli, &Profile::Release)?),
			GenesisBuilder => cmd.genesis_builder = Some(guide_user_to_select_genesis_policy(cli)?),
			GenesisBuilderPreset => cmd.update_genesis_preset(cli)?,
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
			self.get_message().unwrap_or_default()
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

// Locate runtime WASM binary. If it doesn't exist, trigger build.
fn ensure_runtime_binary_exists(
	cli: &mut impl cli::traits::Cli,
	mode: &Profile,
) -> anyhow::Result<PathBuf> {
	let cwd = current_dir().unwrap_or(PathBuf::from("./"));
	let target_path = mode.target_directory(&cwd).join("wbuild");
	let runtime_path = guide_user_to_select_runtime_path(&cwd, cli)?;

	// Return immediately if the user has specified a path to the runtime binary.
	if runtime_path.extension() == Some(OsStr::new("wasm")) {
		return Ok(runtime_path);
	}

	match runtime_binary_path(&target_path, &runtime_path) {
		Ok(binary_path) => Ok(binary_path),
		_ => {
			cli.info("Runtime binary was not found. The runtime will be built locally.")?;
			cli.warning("NOTE: this may take some time...")?;
			build_project(&runtime_path, None, mode, vec!["runtime-benchmarks"], None)?;
			runtime_binary_path(&target_path, &runtime_path).map_err(|e| e.into())
		},
	}
}

fn guide_user_to_select_pallets(
	registry: &PalletExtrinsicsRegistry,
	excluded_pallets: &[String],
	cli: &mut impl cli::traits::Cli,
) -> anyhow::Result<String> {
	// Prompt for pallet search input.
	let input = cli
		.input(r#"Search for pallets by name ("*" to select all)"#)
		.placeholder("balances")
		.required(false)
		.interact()?;

	if input.trim() == ALL_SELECTED {
		return Ok(ALL_SELECTED.to_string());
	}

	// Prompt user to select pallets.
	let pallets = search_for_pallets(registry, excluded_pallets, &input, MAX_PALLET_LIMIT);
	let mut prompt = cli.select("Select the pallets:");
	for pallet in pallets {
		prompt = prompt.item(pallet.clone(), &pallet, "");
	}
	Ok(prompt.interact()?)
}

fn guide_user_to_exclude_pallets(
	registry: &PalletExtrinsicsRegistry,
	cli: &mut impl cli::traits::Cli,
) -> anyhow::Result<String> {
	// Prompt for pallet search input.
	let input = cli
		.input(r#"Search for pallets by name to exclude"#)
		.placeholder("balances")
		.required(false)
		.interact()?;

	// Prompt user to select pallets.
	let pallets = search_for_pallets(registry, &vec![], &input, MAX_PALLET_LIMIT);
	let mut prompt = cli.multiselect("Exclude pallets from benchmarking:").required(false);
	for pallet in pallets {
		prompt = prompt.item(pallet.clone(), &pallet, "");
	}
	Ok(prompt.interact()?.join(","))
}

fn guide_user_to_select_extrinsics(
	pallet: &String,
	registry: &PalletExtrinsicsRegistry,
	cli: &mut impl cli::traits::Cli,
) -> anyhow::Result<String> {
	// Prompt for extrinsic search input.
	let input = cli
		.input(r#"Search for extrinsics by name ("*" to select all)"#)
		.placeholder("transfer")
		.required(false)
		.interact()?;

	if input.trim() == ALL_SELECTED {
		return Ok(ALL_SELECTED.to_string());
	}

	// Prompt user to select extrinsics.
	let extrinsics = search_for_extrinsics(registry, pallet, &input, MAX_EXTRINSIC_LIMIT);
	let mut prompt = cli.multiselect("Select the extrinsics:").required(true);
	for extrinsic in extrinsics {
		prompt = prompt.item(extrinsic.clone(), &extrinsic, "");
	}
	Ok(prompt.interact()?.join(","))
}

fn guide_user_to_select_runtime_path(
	target_path: &Path,
	cli: &mut impl cli::traits::Cli,
) -> anyhow::Result<PathBuf> {
	let mut project_path = get_runtime_path(target_path).or_else(|_| {
		cli.warning(format!(
			"No runtime folder found at {}. Please input the runtime path manually.",
			target_path.display()
		))?;
		guide_user_to_input_runtime_path(cli)
	})?;

	// If there is no TOML file exist, list all directories in the "runtime" folder and prompt the
	// user to select a runtime.
	if project_path.is_dir() && !project_path.join("Cargo.toml").exists() {
		let runtime = guide_user_to_select_runtime(&project_path, cli)?;
		project_path = project_path.join(runtime);
	}
	Ok(project_path)
}

fn guide_user_to_input_runtime_path(cli: &mut impl cli::traits::Cli) -> anyhow::Result<PathBuf> {
	let input = cli
		.input("Please provide the path to the runtime or parachain project.")
		.required(true)
		.default_input("./runtime")
		.placeholder("./runtime")
		.interact()
		.map(PathBuf::from)
		.map_err(anyhow::Error::from)?;
	input.canonicalize().map_err(anyhow::Error::from)
}

fn guide_user_to_select_runtime(
	project_path: &PathBuf,
	cli: &mut impl cli::traits::Cli,
) -> anyhow::Result<PathBuf> {
	let runtimes = fs::read_dir(project_path).expect("No project found");
	let mut prompt = cli.select("Select the runtime:");
	for runtime in runtimes {
		let path = runtime.unwrap().path();
		if !path.is_dir() {
			continue;
		}
		let manifest = from_path(Some(path.as_path()))?;
		let package = manifest.package();
		let name = package.clone().name;
		let description = package.description().unwrap_or_default().to_string();
		prompt = prompt.item(path, &name, &description);
	}
	Ok(prompt.interact()?)
}

fn guide_user_to_select_genesis_policy(
	cli: &mut impl cli::traits::Cli,
) -> anyhow::Result<GenesisBuilderPolicy> {
	let mut prompt = cli
		.select("Select the genesis builder policy:")
		.initial_value(GenesisBuilderPolicy::None.to_string());

	let policies: Vec<(String, String)> = GenesisBuilderPolicy::iter()
		.map(|policy| (policy.to_string(), policy.get_documentation().unwrap().to_string()))
		.collect();
	for (policy, description) in policies {
		prompt = prompt.item(policy.clone(), policy.to_string(), description);
	}
	let input = prompt.interact()?;
	Ok(GenesisBuilderPolicy::from(input))
}

fn guide_user_to_select_genesis_preset(
	cli: &mut impl cli::traits::Cli,
	runtime_path: &PathBuf,
	default_value: &str,
) -> anyhow::Result<String> {
	let spinner = cliclack::spinner();
	spinner.start("Fetching available genesis builder presets of your runtime...");
	let mut prompt = cli
		.select("Select the genesis builder preset:")
		.initial_value(default_value.to_string());
	let preset_names = get_preset_names(runtime_path)?;
	if preset_names.is_empty() {
		return Err(anyhow::anyhow!("No preset found for the runtime"));
	}
	spinner.stop(format!("Found {} genesis builder presets", preset_names.len()));
	for preset in preset_names {
		prompt = prompt.item(preset.to_string(), preset, "");
	}
	Ok(prompt.interact()?)
}

async fn guide_user_to_select_menu_option(
	cmd: &mut BenchmarkPallet,
	cli: &mut impl cli::traits::Cli,
	registry: &mut PalletExtrinsicsRegistry,
) -> anyhow::Result<BenchmarkPalletMenuOption> {
	let mut prompt = cli.select("Select the parameter to update:");

	let mut index = 0;
	for param in BenchmarkPalletMenuOption::iter() {
		if param.is_disabled(cmd, registry).await? {
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
	Ok(prompt.interact()?)
}

fn is_selected_all(s: &String) -> bool {
	s == &ALL_SELECTED.to_string() || s.is_empty()
}

// Add a more relaxed parsing for pallet names by allowing pallet directory names with `-` to be
// used like crate names with `_`
fn parse_pallet_name(pallet: &str) -> std::result::Result<String, String> {
	Ok(pallet.replace("-", "_"))
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::cli::MockCli;
	use duct::cmd;
	use tempfile::tempdir;

	#[tokio::test]
	async fn benchmark_pallet_works() -> anyhow::Result<()> {
		let runtime_path = get_mock_runtime(true);
		let mut cli = MockCli::new();
		cli = expect_pallet_benchmarking_intro(cli);
		cli = expect_select_genesis_policy(cli, 1);
		cli = expect_select_genesis_preset(cli, &runtime_path, 0)
			.expect_warning("NOTE: this may take some time...")
			.expect_info("Benchmarking extrinsic weights of selected pallets...");

		let mut cmd =
			BenchmarkPallet { skip_menu: true, skip_confirm: false, ..Default::default() };
		cmd.execute(&mut cli).await?;

		// Verify the printed command.
		let mut command_output = cmd.display();
		command_output.push_str(" --skip");
		cli = cli.expect_info(command_output);
		cli = cli.expect_outro("Benchmark completed successfully!");
		cmd.execute(&mut cli).await?;
		cli.verify()
	}

	#[tokio::test]
	async fn benchmark_pallet_without_runtime_benchmarks_feature_fails() -> anyhow::Result<()> {
		let mut cli = MockCli::new();
		cli = expect_pallet_benchmarking_intro(cli);
		cli = expect_select_genesis_policy(cli, 0);
		cli = cli.expect_outro_cancel(
	        "Failed to run benchmarking: Invalid input: Could not call runtime API to Did not find the benchmarking metadata. \
	        This could mean that you either did not build the node correctly with the `--features runtime-benchmarks` flag, \
			or the chain spec that you are using was not created by a node that was compiled with the flag: \
			Other: Exported method Benchmark_benchmark_metadata is not found"
		);

		BenchmarkPallet {
			runtime: Some(get_mock_runtime(false)),
			skip_menu: true,
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
		cli = expect_select_genesis_policy(cli, 0);
		cli = cli.expect_outro_cancel("Failed to run benchmarking: Invalid input: No benchmarks found which match your input.");

		BenchmarkPallet {
			runtime: Some(get_mock_runtime(true)),
			skip_menu: true,
			..Default::default()
		}
		.execute(&mut cli)
		.await?;
		cli.verify()
	}

	#[test]
	fn guide_user_to_select_runtime_works() -> anyhow::Result<()> {
		let temp_dir = tempdir()?;
		let runtimes = ["runtime-1", "runtime-2", "runtime-3"];
		let runtime_path = temp_dir.path().join("runtime");
		let runtime_items = runtimes.map(|runtime| (runtime.to_string(), "".to_string())).to_vec();

		// Found runtimes in the specified runtime path.
		let mut cli = MockCli::new();
		cli = cli.expect_select("Select the runtime:", Some(true), true, Some(runtime_items), 0);

		fs::create_dir(&runtime_path)?;
		for runtime in runtimes {
			cmd("cargo", ["new", runtime, "--bin"]).dir(&runtime_path).run()?;
		}
		guide_user_to_select_runtime(&runtime_path, &mut cli)?;
		cli.verify()
	}

	#[test]
	fn guide_user_to_select_runtime_path_works() -> anyhow::Result<()> {
		let temp_dir = tempdir()?;
		let temp_path = temp_dir.path().to_path_buf();
		let runtime_path = temp_dir.path().join("runtimes");

		// No runtime path found, ask for manual input from user.
		let mut cli = MockCli::new();
		let runtime_binary_path = temp_path.join("dummy.wasm");
		cli = cli.expect_warning(format!(
			"No runtime folder found at {}. Please input the runtime path manually.",
			temp_path.display()
		));
		cli = cli.expect_input(
			"Please provide the path to the runtime or parachain project.",
			runtime_binary_path.to_str().unwrap().to_string(),
		);
		fs::File::create(runtime_binary_path)?;
		guide_user_to_select_runtime_path(&temp_path, &mut cli)?;
		cli.verify()?;

		// Runtime folder found and not a Rust project, select from existing runtimes.
		fs::create_dir(&runtime_path)?;
		let runtimes = ["runtime-1", "runtime-2", "runtime-3"];
		let runtime_items = runtimes.map(|runtime| (runtime.to_string(), "".to_string())).to_vec();
		cli = MockCli::new();
		cli = cli.expect_select("Select the runtime:", Some(true), true, Some(runtime_items), 0);
		for runtime in runtimes {
			cmd("cargo", ["new", runtime, "--bin"]).dir(&runtime_path).run()?;
		}
		guide_user_to_select_runtime_path(&temp_path, &mut cli)?;

		cli.verify()
	}

	#[test]
	fn guide_user_to_select_genesis_policy_works() -> anyhow::Result<()> {
		// Select genesis builder policy `none`.
		let mut cli = MockCli::new();
		cli = expect_select_genesis_policy(cli, 0);

		guide_user_to_select_genesis_policy(&mut cli)?;
		cli.verify()?;

		// Select genesis builder policy `runtime`.
		let runtime_path = get_mock_runtime(true);
		cli = MockCli::new();
		cli = expect_select_genesis_policy(cli, 1);
		cli = expect_select_genesis_preset(cli, &runtime_path, 0);

		guide_user_to_select_genesis_policy(&mut cli)?;
		guide_user_to_select_genesis_preset(&mut cli, &runtime_path, "development")?;
		cli.verify()
	}

	#[test]
	fn guide_user_to_select_genesis_preset_works() -> anyhow::Result<()> {
		let runtime_path = get_mock_runtime(false);
		let mut cli = MockCli::new();
		cli = expect_select_genesis_preset(cli, &runtime_path, 0);
		guide_user_to_select_genesis_preset(&mut cli, &runtime_path, "development")?;
		cli.verify()
	}

	#[tokio::test]
	async fn guide_user_to_select_pallets_works() -> anyhow::Result<()> {
		let mut cli = MockCli::new();
		let runtime_path = get_mock_runtime(true);
		let binary_path = check_omni_bencher_and_prompt(&mut cli, &crate::cache()?, true).await?;
		let registry = load_pallet_extrinsics(&runtime_path, binary_path.as_path()).await?;
		let prompt = r#"Search for pallets by name ("*" to select all)"#;

		// Select all pallets.
		let mut cli = MockCli::new();
		cli = cli.expect_input(prompt, ALL_SELECTED.to_string());
		let input = guide_user_to_select_pallets(&registry, &[], &mut cli)?;
		assert_eq!(input, ALL_SELECTED.to_string());
		cli.verify()?;

		// Search for pallets.
		let input = "pallet_timestamp";
		let pallets = search_for_pallets(&registry, &[], &input, MAX_PALLET_LIMIT);
		cli = MockCli::new().expect_input(prompt, input.to_string()).expect_select(
			"Select the pallets:",
			Some(true),
			true,
			Some(
				pallets
					.into_iter()
					.map(|pallet| (pallet, Default::default()))
					.take(MAX_PALLET_LIMIT)
					.collect(),
			),
			0,
		);
		guide_user_to_select_pallets(&registry, &vec![], &mut cli)?;
		cli.verify()
	}

	#[tokio::test]
	async fn guide_user_to_select_extrinsics_works() -> anyhow::Result<()> {
		let mut cli = MockCli::new();
		let runtime_path = get_mock_runtime(true);
		let binary_path = check_omni_bencher_and_prompt(&mut cli, &crate::cache()?, true).await?;
		let registry = load_pallet_extrinsics(&runtime_path, binary_path.as_path()).await?;
		let prompt = r#"Search for extrinsics by name ("*" to select all)"#;

		// Select all extrinsics.
		let mut cli = MockCli::new();
		cli = cli.expect_input(prompt, ALL_SELECTED.to_string());
		let input =
			guide_user_to_select_extrinsics(&"pallet_timestamp".to_string(), &registry, &mut cli)?;
		assert_eq!(input, ALL_SELECTED.to_string());
		cli.verify()?;

		// Search for pallets.
		let input = "on_finalize";
		let extrinsics = search_for_extrinsics(
			&registry,
			&"pallet_timestamp".to_string(),
			&input,
			MAX_EXTRINSIC_LIMIT,
		);
		cli = MockCli::new()
			.expect_input(prompt, input.to_string())
			.expect_multiselect::<String>(
				"Select the extrinsics:",
				Some(true),
				true,
				Some(
					extrinsics
						.into_iter()
						.map(|extrinsic| (extrinsic, Default::default()))
						.take(MAX_EXTRINSIC_LIMIT)
						.collect(),
				),
			);
		guide_user_to_select_extrinsics(&"pallet_timestamp".to_string(), &registry, &mut cli)?;
		cli.verify()
	}

	#[tokio::test]
	async fn menu_option_is_disabled_works() -> anyhow::Result<()> {
		use BenchmarkPalletMenuOption::*;
		let mut cli = MockCli::new();
		let runtime_path = get_mock_runtime(true);
		let binary_path = check_omni_bencher_and_prompt(&mut cli, &crate::cache()?, true).await?;
		let registry = load_pallet_extrinsics(&runtime_path, binary_path.as_path()).await?;

		let cmd = BenchmarkPallet {
			runtime: Some(get_mock_runtime(false)),
			pallet: Some(ALL_SELECTED.to_string()),
			extrinsic: Some(ALL_SELECTED.to_string()),
			genesis_builder: Some(GenesisBuilderPolicy::None),
			..Default::default()
		};
		assert!(!GenesisBuilder.is_disabled(&cmd, &registry).await?);
		assert!(GenesisBuilderPreset.is_disabled(&cmd, &registry).await?);
		assert!(Extrinsics.is_disabled(&cmd, &registry).await?);
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
			genesis_builder_preset: GENESIS_BUILDER_DEV_PRESET.to_string(),
			steps: 50,
			repeat: 20,
			worst_case_map_values: 1000000,
			database_cache_size: 1024,
			additional_trie_layers: 2,
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
		for (option, value) in options.into_iter() {
			cli = cli.expect_input(
				format!(
					r#"Provide value to the parameter "{}""#,
					option.get_message().unwrap_or_default()
				),
				value.to_string(),
			);
		}
		for (option, _) in options.into_iter() {
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
	fn get_runtime_argument_works() -> anyhow::Result<()> {
		assert_eq!(
			BenchmarkPallet { runtime: Some(get_mock_runtime(false)), ..Default::default() }
				.runtime()
				.unwrap(),
			&get_mock_runtime(false)
		);
		assert!(matches!(BenchmarkPallet::default().runtime(), Err(message)
			if message.to_string().contains("No runtime found")
		));
		Ok(())
	}

	fn expect_pallet_benchmarking_intro(cli: MockCli) -> MockCli {
		cli.expect_intro("Benchmarking your pallets").expect_warning(
			"NOTE: the `pop bench pallet` is not yet battle tested - double check the results.",
		)
	}

	fn expect_select_genesis_policy(cli: MockCli, item: usize) -> MockCli {
		let policies: Vec<(String, String)> = GenesisBuilderPolicy::iter()
			.map(|policy| (policy.to_string(), policy.get_documentation().unwrap().to_string()))
			.collect();
		cli.expect_select(
			"Select the genesis builder policy:",
			Some(true),
			true,
			Some(policies),
			item,
		)
	}

	fn expect_select_genesis_preset(cli: MockCli, runtime_path: &PathBuf, item: usize) -> MockCli {
		let preset_names = get_preset_names(runtime_path)
			.unwrap()
			.into_iter()
			.map(|preset| (preset, String::default()))
			.collect();
		cli.expect_select(
			"Select the genesis builder preset:",
			Some(true),
			true,
			Some(preset_names),
			item,
		)
	}

	// Construct the path to the mock runtime WASM file.
	fn get_mock_runtime(with_benchmark_features: bool) -> std::path::PathBuf {
		let path = format!(
			"../../tests/runtimes/{}.wasm",
			if with_benchmark_features { "base_parachain_benchmark" } else { "base_parachain" }
		);
		current_dir().unwrap().join(path).canonicalize().unwrap()
	}
}
