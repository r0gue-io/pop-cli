// SPDX-License-Identifier: GPL-3.0

use crate::{
	cli::{
		self, spinner,
		traits::{Confirm, Input, MultiSelect, Select, Spinner},
	},
	common::{
		bench::{check_omni_bencher_and_prompt, overwrite_weight_file_command},
		builds::guide_user_to_select_profile,
		prompt::display_message,
		runtime::{
			Feature, ensure_runtime_binary_exists, guide_user_to_select_genesis_policy,
			guide_user_to_select_genesis_preset,
		},
	},
};
use clap::Args;
use pop_chains::{
	GENESIS_BUILDER_DEV_PRESET, GenesisBuilderPolicy, PalletExtrinsicsRegistry,
	generate_pallet_benchmarks, load_pallet_extrinsics, utils::helpers::get_preset_names,
};
use pop_common::get_relative_or_absolute_path;
use serde::{Deserialize, Serialize};
use std::{
	collections::BTreeMap,
	env::current_dir,
	ffi::OsStr,
	fs,
	path::{Path, PathBuf},
};
use strum::{EnumMessage, IntoEnumIterator};
use strum_macros::{EnumIter, EnumMessage as EnumMessageDerive};
use tempfile::tempdir;

const ALL_SELECTED: &str = "*";
const ALL_SELECTED_TEXT: &str = "All selected";
const DEFAULT_BENCH_FILE: &str = "pop-bench.toml";
const ARGUMENT_NO_VALUE: &str = "None";

type PalletExtrinsicItem = (String, String);

#[derive(Args, Serialize, Deserialize, Clone, PartialEq, Eq, Debug)]
pub(crate) struct BenchmarkPallet {
	/// Select a FRAME Pallets to benchmark, or `*` for all (in which case `extrinsic` must be
	/// `*`).
	#[arg(short, long, alias = "pallet", num_args = 1.., value_delimiter = ',', value_parser = parse_pallet_name, default_value_if("all", "true", Some("*".into())))]
	pub pallets: Vec<String>,

	/// Select an extrinsic inside the pallet to benchmark, or `*` for all.
	#[arg(short, long, default_value_if("all", "true", Some("*".into())))]
	extrinsic: Option<String>,

	/// Comma separated list of pallets that should be excluded from the benchmark.
	#[arg(long, value_parser, num_args = 1.., value_delimiter = ',')]
	exclude_pallets: Vec<String>,

	/// Comma separated list of `pallet::extrinsic` combinations that should not be run.
	///
	/// Example: `frame_system::remark,pallet_balances::transfer_keep_alive`
	#[arg(long, value_parser, num_args = 1.., value_delimiter = ',')]
	exclude_extrinsics: Vec<String>,

	/// Run benchmarks for all pallets and extrinsics.
	///
	/// This is equivalent to running `--pallet * --extrinsic *`.
	#[arg(long)]
	all: bool,

	/// Select how many samples we should take across the variable components.
	#[arg(short, long, default_value_t = 50)]
	steps: u32,

	/// Indicates lowest values for each of the component ranges.
	#[arg(long = "low", value_delimiter = ',')]
	lowest_range_values: Vec<u32>,

	/// Indicates highest values for each of the component ranges.
	#[arg(long = "high", value_delimiter = ',')]
	highest_range_values: Vec<u32>,

	/// Select how many repetitions of this benchmark should run from within the wasm.
	#[arg(short, long, default_value_t = 20)]
	repeat: u32,

	/// Select how many repetitions of this benchmark should run from the client.
	///
	/// NOTE: Using this alone may give slower results, but will afford you maximum Wasm memory.
	#[arg(long, default_value_t = 1)]
	external_repeat: u32,

	/// Write the raw results in JSON format into the given file.
	#[serde(skip_serializing)]
	#[arg(long)]
	json_file: Option<PathBuf>,

	/// Don't print the median-slopes linear regression analysis.
	#[arg(long)]
	no_median_slopes: bool,

	/// Don't print the min-squares linear regression analysis.
	#[arg(long)]
	no_min_squares: bool,

	/// Output the benchmarks to a Rust file at the given path.
	#[serde(skip_serializing)]
	#[arg(long)]
	output: Option<PathBuf>,

	/// Path to Handlebars template file used for outputting benchmark results. (Optional)
	#[serde(skip_serializing)]
	#[arg(long)]
	template: Option<PathBuf>,

	/// Which analysis function to use when outputting benchmarks:
	/// * min-squares (default)
	/// * median-slopes
	/// * max (max of min squares and median slopes for each value)
	#[arg(long)]
	output_analysis: Option<String>,

	/// Which analysis function to use when analyzing measured proof sizes.
	#[arg(long, default_value("median-slopes"))]
	output_pov_analysis: Option<String>,

	/// Set the heap pages while running benchmarks. If not set, the default value from the client
	/// is used.
	#[arg(long)]
	heap_pages: Option<u64>,

	/// Disable verification logic when running benchmarks.
	#[arg(long)]
	no_verify: bool,

	/// Display and run extra benchmarks that would otherwise not be needed for weight
	/// construction.
	#[arg(long)]
	extra: bool,

	/// Path to the runtime project or binary.
	#[serde(skip_serializing)]
	#[arg(long)]
	runtime: Option<PathBuf>,

	// Only used internally to save the runtime binary path.
	#[clap(skip)]
	#[serde(skip_serializing)]
	runtime_binary: Option<PathBuf>,

	/// Do not fail if there are unknown but also unused host functions in the runtime.
	#[arg(long)]
	allow_missing_host_functions: bool,

	/// How to construct the genesis state.
	#[arg(long, alias = "genesis-builder-policy")]
	genesis_builder: Option<GenesisBuilderPolicy>,

	/// The preset that we expect to find in the GenesisBuilder runtime API.
	///
	/// This can be useful when a runtime has a dedicated benchmarking preset instead of using the
	/// default one.
	#[arg(long, default_value = GENESIS_BUILDER_DEV_PRESET)]
	genesis_builder_preset: String,

	/// Limit the memory the database cache can use.
	#[arg(long = "db-cache", value_name = "MiB", default_value_t = 1024)]
	database_cache_size: u32,

	/// List and print available benchmarks in a csv-friendly format.
	#[arg(long)]
	list: bool,

	/// If enabled, the storage info is not displayed in the output next to the analysis.
	///
	/// This is independent of the storage info appearing in the *output file*. Use a Handlebar
	/// template for that purpose.
	#[arg(long)]
	no_storage_info: bool,

	/// The assumed default maximum size of any `StorageMap`.
	///
	/// When the maximum size of a map is not defined by the runtime developer,
	/// this value is used as a worst case scenario. It will affect the calculated worst case
	/// PoV size for accessing a value in a map, since the PoV will need to include the trie
	/// nodes down to the underlying value.
	#[clap(long = "map-size", default_value = "1000000")]
	worst_case_map_values: u32,

	/// Adjust the PoV estimation by adding additional trie layers to it.
	///
	/// This should be set to `log16(n)` where `n` is the number of top-level storage items in the
	/// runtime, e.g. `StorageMap`s and `StorageValue`s. A value of 2 to 3 is usually sufficient.
	/// Each layer will result in an additional 495 bytes PoV per distinct top-level access.
	/// Therefore, multiple `StorageMap` accesses only suffer from this increase once. The exact
	/// number of storage items depends on the runtime and the deployed pallets.
	#[clap(long, default_value = "2")]
	additional_trie_layers: u8,

	/// Do not enable proof recording during time benchmarking.
	///
	/// By default, proof recording is enabled during benchmark execution. This can slightly
	/// inflate the resulting time weights. For parachains using PoV-reclaim, this is typically the
	/// correct setting. Chains that ignore the proof size dimension of weight (e.g. relay chain,
	/// solo-chains) can disable proof recording to get more accurate results.
	#[arg(long)]
	disable_proof_recording: bool,

	/// If enabled, no prompt will be shown for updating additional parameters.
	#[arg(long)]
	skip_parameters: bool,

	/// Automatically source the needed binary required without prompting for confirmation.
	#[clap(short = 'y', long)]
	skip_confirm: bool,

	/// Avoid rebuilding the runtime if there is an existing runtime binary.
	#[clap(short = 'n', long)]
	no_build: bool,

	/// Output file of the benchmark parameters.
	#[clap(short = 'f', long)]
	#[serde(skip_serializing)]
	bench_file: Option<PathBuf>,
}

impl Default for BenchmarkPallet {
	fn default() -> Self {
		Self {
			pallets: vec![],
			extrinsic: None,
			exclude_pallets: vec![],
			exclude_extrinsics: vec![],
			all: false,
			steps: 50,
			lowest_range_values: vec![],
			highest_range_values: vec![],
			repeat: 20,
			external_repeat: 1,
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
			runtime_binary: None,
			allow_missing_host_functions: false,
			genesis_builder: None,
			genesis_builder_preset: GENESIS_BUILDER_DEV_PRESET.to_string(),
			database_cache_size: 1024,
			list: false,
			no_storage_info: false,
			worst_case_map_values: 1000000,
			additional_trie_layers: 2,
			disable_proof_recording: false,
			skip_parameters: false,
			skip_confirm: false,
			no_build: false,
			bench_file: None,
		}
	}
}

impl BenchmarkPallet {
	pub async fn execute(
		&mut self,
		cli: &mut impl cli::traits::Cli,
	) -> anyhow::Result<serde_json::Value> {
		// If `all` is provided, we override the value of `pallet` and `extrinsic` to select all.
		if self.all {
			self.pallets = vec![ALL_SELECTED.to_string()];
			self.extrinsic = Some(ALL_SELECTED.to_string());
			self.all = false;
		}

		let mut registry: PalletExtrinsicsRegistry = BTreeMap::default();

		cli.intro(if self.list {
			"Listing available pallets and extrinsics"
		} else {
			"Benchmarking your pallets"
		})?;
		cli.warning(
			"NOTE: the `pop bench pallet` is not yet battle tested - double check the results.",
		)?;

		// If bench file is provided, load the provided parameters in the file.
		if let Some(bench_file) = self.bench_file.clone() {
			cli.info(format!(
				"Benchmarking parameter file found at {:?}. Loading parameters...",
				bench_file.display()
			))?;
			*self = VersionedBenchmarkPallet::try_from(bench_file.as_path())?.parameters();
			self.bench_file = Some(bench_file);
		}
		let original_cmd = self.clone();

		// No runtime path provided, auto-detect the runtime WASM binary. If not found, build
		// the runtime. If the runtime path is a directory, it rebuilds the runtime.
		let invalid_runtime = match self.runtime {
			Some(ref r) => {
				if r.is_file() {
					self.runtime_binary = Some(r.clone());
				}
				r.is_dir()
			},
			None => true,
		};
		if invalid_runtime && let Err(e) = self.update_runtime_path(cli).await {
			return display_message(&e.to_string(), false, cli);
		}

		if self.list {
			// Without overriding the genesis builder policy, listing will fail for a runtime
			// that is not built with the `runtime-benchmarks` feature.
			self.genesis_builder = Some(GenesisBuilderPolicy::None);
			if let Err(e) = self.run(cli) {
				return display_message(&e.to_string(), false, cli);
			}
			return display_message("All pallets and extrinsics listed!", true, cli);
		}

		// No genesis builder, prompts user to select the genesis builder policy.
		if self.genesis_builder.is_none() {
			let runtime_path = self.runtime_binary()?.clone();
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
		if self.pallets.is_empty() &&
			let Err(e) = self.update_pallets(cli, &mut registry).await
		{
			return display_message(&e.to_string(), false, cli);
		};
		// No extrinsic provided, prompts user to select the extrinsics fetched from runtime.
		if self.extrinsic.is_none() {
			self.update_extrinsics(cli, &mut registry).await?;
		}

		// Only prompt user to update additional parameter configuration when `skip_parameters` is
		// not provided.
		if !self.skip_parameters &&
			cli.confirm("Would you like to update any additional configurations?")
				.initial_value(false)
				.interact()?
		{
			self.ensure_pallet_registry(cli, &mut registry).await?;
			loop {
				let option = guide_user_to_select_menu_option(self, cli).await?;
				match option.update_arguments(self, &mut registry, cli).await {
					Ok(true) => break,
					Ok(false) => continue,
					Err(e) => cli.error(e)?,
				}
			}
		}

		// Prompt user to update output path of the benchmarking results.
		if self.pallets.len() <= 1 && self.output.is_none() {
			let input = cli
				.input("Provide the output path for benchmark results (optional).")
				.required(false)
				.placeholder("./weights.rs")
				.interact()?;
			self.output = if !input.is_empty() { Some(input.into()) } else { None };
		}

		// Prompt user to save benchmarking parameters to output file if there are changes made.
		if let Some(bench_file) =
			guide_user_to_update_bench_file_path(self, cli, self != &original_cmd)?
		{
			let toml_output = toml::to_string(&VersionedBenchmarkPallet::from(self.clone()))?;
			fs::write(&bench_file, toml_output)?;
			cli.info(format!("Parameters saved successfully to {:?}", bench_file.display()))?;
		}

		cli.warning("NOTE: this may take some time...")?;
		cli.info("Benchmarking extrinsic weights of selected pallets...")?;
		let result = self.run(cli);

		// Display the benchmarking command.
		cli.info(self.display(cli.is_json()))?;
		if let Err(e) = result {
			return display_message(&e.to_string(), false, cli);
		}
		display_message("Benchmark completed successfully!", true, cli)
	}

	fn run(&mut self, cli: &mut impl cli::traits::Cli) -> anyhow::Result<()> {
		if let Some(original_weight_path) = self.output.clone() {
			if original_weight_path.extension().is_some() {
				self.run_with_weight_file(cli, original_weight_path)?;
			} else {
				self.run_with_weight_dir(cli, original_weight_path)?;
			}
		} else {
			generate_pallet_benchmarks(self.collect_run_arguments(cli.is_json()))?;
		}
		Ok(())
	}

	fn run_with_weight_file(
		&mut self,
		cli: &mut impl cli::traits::Cli,
		weight_path: PathBuf,
	) -> anyhow::Result<()> {
		let temp_dir = tempdir()?;
		let temp_file_path = temp_dir.path().join("temp_weights.rs");
		self.output = Some(temp_file_path.clone());

		generate_pallet_benchmarks(self.collect_run_arguments(cli.is_json()))?;
		console::Term::stderr().clear_last_lines(1)?;
		cli.info(format!("Weight file is generated to {:?}", weight_path.display()))?;

		// Restore the original weight path.
		self.output = Some(weight_path.clone());
		// Overwrite the weight files with the correct executed command.
		overwrite_weight_file_command(
			&temp_file_path,
			&weight_path,
			&self.collect_display_arguments(cli.is_json()),
		)?;
		Ok(())
	}

	fn run_with_weight_dir(
		&mut self,
		cli: &mut impl cli::traits::Cli,
		weight_path: PathBuf,
	) -> anyhow::Result<()> {
		let temp_dir = tempdir()?;
		let temp_dir_path = temp_dir.path().to_path_buf();
		self.output = Some(temp_dir_path.clone());

		generate_pallet_benchmarks(self.collect_run_arguments(cli.is_json()))?;
		console::Term::stderr()
			.clear_last_lines(fs::read_dir(temp_dir_path.clone()).iter().count() + 1)?;

		// Restore the original weight path.
		self.output = Some(weight_path.clone());
		// Overwrite the weight files with the correct executed command.
		let mut info = String::default();
		for entry in fs::read_dir(temp_dir_path)? {
			let entry = entry?;
			let path = entry.path();
			let original_path = weight_path.join(entry.file_name());
			overwrite_weight_file_command(
				&path,
				&original_path,
				&self.collect_display_arguments(cli.is_json()),
			)?;
			info.push_str(&format!("Created file: {:?}\n", original_path));
		}
		cli.info(info)?;
		Ok(())
	}

	fn display(&self, json: bool) -> String {
		self.collect_display_arguments(json).join(" ")
	}

	fn collect_display_arguments(&self, json: bool) -> Vec<String> {
		let default_values = Self::default();
		let mut args = vec!["pop".to_string(), "bench".to_string(), "pallet".to_string()];
		let mut arguments = self.collect_arguments(json);
		if self.skip_parameters && self.skip_parameters != default_values.skip_parameters {
			arguments.push("--skip-parameters".to_string());
		}
		if self.skip_confirm {
			arguments.push("-y".to_string());
		}
		if self.no_build {
			arguments.push("-n".to_string());
		}
		if let Some(ref runtime) = self.runtime {
			args.push(format!("--runtime={}", runtime.display()));
		}
		args.extend(arguments);
		args
	}

	fn collect_run_arguments(&self, json: bool) -> Vec<String> {
		let mut arguments = self.collect_arguments(json);
		if let Some(ref binary) = self.runtime_binary {
			arguments.push(format!("--runtime={}", binary.display()));
		}
		arguments
	}

	fn collect_arguments(&self, json: bool) -> Vec<String> {
		let default_values = Self::default();
		let mut args = vec![];

		if self.list {
			args.push("--list".to_string());
		}

		if !self.pallets.is_empty() {
			args.push(format!("--pallets={}", self.pallets.join(",")));
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

		if !self.exclude_extrinsics.is_empty() {
			args.push(format!("--exclude-extrinsics={}", self.exclude_extrinsics.join(",")));
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

		if self.repeat != default_values.repeat {
			args.push(format!("--repeat={}", self.repeat));
		}
		if self.external_repeat != default_values.external_repeat {
			args.push(format!("--external-repeat={}", self.external_repeat));
		}
		if self.database_cache_size != default_values.database_cache_size {
			args.push(format!("--db-cache={}", self.database_cache_size));
		}
		if self.worst_case_map_values != default_values.worst_case_map_values {
			args.push(format!("--map-size={}", self.worst_case_map_values));
		}
		if self.additional_trie_layers != default_values.additional_trie_layers {
			args.push(format!("--additional-trie-layers={}", self.additional_trie_layers));
		}
		if json {
			args.push("--json".to_string());
		}
		if let Some(ref json_file) = self.json_file {
			args.push(format!("--json-file={}", json_file.display()));
		}
		if self.no_median_slopes && self.no_median_slopes != default_values.no_median_slopes {
			args.push("--no-median-slopes".to_string());
		}
		if self.no_min_squares && self.no_min_squares != default_values.no_min_squares {
			args.push("--no-min-squares".to_string());
		}
		if self.no_storage_info && self.no_storage_info != default_values.no_storage_info {
			args.push("--no-storage-info".to_string());
		}
		if let Some(ref output) = self.output {
			let relative_output_path = get_relative_path(output.as_path());
			args.push(format!("--output={}", relative_output_path.display()));
		}
		if let Some(ref template) = self.template {
			args.push(format!("--template={}", template.display()));
		}
		if self.output_analysis != default_values.output_analysis &&
			let Some(ref output_analysis) = self.output_analysis
		{
			args.push(format!("--output-analysis={}", output_analysis));
		}
		if self.output_pov_analysis != default_values.output_pov_analysis &&
			let Some(ref output_pov_analysis) = self.output_pov_analysis
		{
			args.push(format!("--output-pov-analysis={}", output_pov_analysis));
		}
		if let Some(ref heap_pages) = self.heap_pages {
			args.push(format!("--heap-pages={}", heap_pages));
		}
		if self.no_verify && self.no_verify != default_values.no_verify {
			args.push("--no-verify".to_string());
		}
		if self.extra && self.extra != default_values.extra {
			args.push("--extra".to_string());
		}

		if self.allow_missing_host_functions &&
			self.allow_missing_host_functions != default_values.allow_missing_host_functions
		{
			args.push("--allow-missing-host-functions".to_string());
		}
		if self.genesis_builder != default_values.genesis_builder &&
			let Some(ref genesis_builder) = self.genesis_builder
		{
			args.push(format!("--genesis-builder={}", genesis_builder));
			if genesis_builder == &GenesisBuilderPolicy::Runtime &&
				self.genesis_builder_preset != default_values.genesis_builder_preset
			{
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
			let runtime_path = self.runtime_binary()?;
			let spinner = spinner();
			let binary_path =
				check_omni_bencher_and_prompt(cli, &spinner, self.skip_confirm).await?;

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
		self.pallets = guide_user_to_select_pallets(registry, &self.exclude_pallets, cli)?;
		Ok(())
	}

	async fn update_extrinsics(
		&mut self,
		cli: &mut impl cli::traits::Cli,
		registry: &mut PalletExtrinsicsRegistry,
	) -> anyhow::Result<()> {
		self.ensure_pallet_registry(cli, registry).await?;
		// Not allow selecting extrinsics when multiple pallets are selected.
		if pallet_selected(&self.pallets, &self.exclude_pallets, ALL_SELECTED) {
			self.extrinsic = Some(ALL_SELECTED.to_string());
		} else {
			self.extrinsic = Some(guide_user_to_select_extrinsics(
				&self.pallets,
				&self.exclude_pallets,
				&self.exclude_extrinsics,
				registry,
				cli,
			)?);
		}
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

	async fn update_excluded_extrinsics(
		&mut self,
		cli: &mut impl cli::traits::Cli,
		registry: &mut PalletExtrinsicsRegistry,
	) -> anyhow::Result<()> {
		self.ensure_pallet_registry(cli, registry).await?;
		let excluded_pallet_extrinsics =
			guide_user_to_exclude_extrinsics(registry, &self.pallets, &self.exclude_pallets, cli)?;
		self.exclude_extrinsics =
			excluded_pallet_extrinsics.into_iter().filter(|s| !s.is_empty()).collect();
		Ok(())
	}

	async fn update_runtime_path(&mut self, cli: &mut impl cli::traits::Cli) -> anyhow::Result<()> {
		let profile = guide_user_to_select_profile(cli)?;
		let (binary_path, project_path) = ensure_runtime_binary_exists(
			cli,
			&get_current_directory(),
			&profile,
			&[Feature::Benchmark],
			!self.no_build,
			false,
			&self.runtime,
			None,
		)
		.await?;
		self.runtime_binary = Some(binary_path);
		self.runtime = Some(get_relative_path(project_path.as_path()));
		Ok(())
	}

	fn update_template_path(&mut self, cli: &mut impl cli::traits::Cli) -> anyhow::Result<()> {
		let input = cli
			.input("Provide path to the custom template for generated weight files (optional)")
			.required(false)
			.interact()?;
		let path: PathBuf = input.into();
		if !path.is_file() {
			return Err(anyhow::anyhow!("Template path does not exist or is a directory"));
		}
		self.template = Some(path);
		Ok(())
	}

	fn runtime(&self) -> anyhow::Result<&PathBuf> {
		self.runtime.as_ref().ok_or_else(|| anyhow::anyhow!("No runtime found"))
	}

	fn runtime_binary(&self) -> anyhow::Result<&PathBuf> {
		self.runtime_binary
			.as_ref()
			.ok_or_else(|| anyhow::anyhow!("No runtime binary found"))
	}

	fn extrinsic(&self) -> anyhow::Result<&String> {
		self.extrinsic.as_ref().ok_or_else(|| anyhow::anyhow!("No extrinsic provided"))
	}

	/// Check the format of `exclude_extrinsics` is following the right format `pallet:extrinsic`.
	fn check_excluded_extrinsics(&self) -> anyhow::Result<()> {
		for e in &self.exclude_extrinsics {
			let splits = e.split("::").collect::<Vec<_>>();
			if splits.len() != 2 {
				return Err(anyhow::anyhow!(
					"Invalid argument for '--exclude-extrinsics'. Expected format: 'pallet::extrinsic' but got '{}'",
					e
				));
			}
		}
		Ok(())
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
	/// Comma separated list of extrinsics that should be excluded from the benchmark
	#[strum(message = "Excluded extrinsics")]
	ExcludedExtrinsics,
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
	/// Path to the custom weight file template
	#[strum(message = "Weight file template")]
	WeightFileTemplate,
	#[strum(message = "> Save all parameter changes and continue")]
	SaveAndContinue,
}

impl BenchmarkPalletMenuOption {
	// Check if the menu option is disabled. If disabled, the menu option is not displayed in the
	// menu.
	fn is_disabled(self, cmd: &BenchmarkPallet) -> anyhow::Result<bool> {
		use BenchmarkPalletMenuOption::*;
		match self {
			GenesisBuilder | GenesisBuilderPreset => {
				let presets = get_preset_names(cmd.runtime_binary()?)?;
				// If there are no presets available, disable the preset builder options.
				if presets.is_empty() {
					return Ok(true);
				}
				if self == GenesisBuilderPreset {
					return Ok(cmd.genesis_builder == Some(GenesisBuilderPolicy::None));
				}
				Ok(false)
			},
			// If there are multiple pallets provided, disable the weight file template.
			WeightFileTemplate => Ok(cmd.pallets.len() > 1),
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
			Pallets =>
				if pallet_selected(&cmd.pallets, &cmd.exclude_pallets, ALL_SELECTED) {
					ALL_SELECTED_TEXT.to_string()
				} else {
					cmd.pallets.join(",")
				},
			Extrinsics => self.get_joined_string(cmd.extrinsic()?),
			ExcludedPallets =>
				if cmd.exclude_pallets.is_empty() {
					ARGUMENT_NO_VALUE.to_string()
				} else {
					cmd.check_excluded_extrinsics()?;
					cmd.exclude_pallets.join(",")
				},
			ExcludedExtrinsics =>
				if cmd.exclude_extrinsics.is_empty() {
					ARGUMENT_NO_VALUE.to_string()
				} else {
					cmd.exclude_extrinsics.join(",")
				},
			Runtime => format!("{}", get_relative_path(cmd.runtime()?).display()),
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
			WeightFileTemplate =>
				if let Some(ref template) = cmd.template {
					format!("{}", get_relative_path(template).display())
				} else {
					ARGUMENT_NO_VALUE.to_string()
				},
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
			ExcludedExtrinsics => cmd.update_excluded_extrinsics(cli, registry).await?,
			Runtime => cmd.update_runtime_path(cli).await?,
			GenesisBuilder =>
				cmd.genesis_builder =
					Some(guide_user_to_select_genesis_policy(cli, &cmd.genesis_builder)?),
			GenesisBuilderPreset => {
				cmd.genesis_builder_preset = guide_user_to_select_genesis_preset(
					cli,
					cmd.runtime_binary()?,
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
			WeightFileTemplate => cmd.update_template_path(cli)?,
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
		let parsed_default_value = default_value.trim().parse()?;
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
			return ARGUMENT_NO_VALUE.to_string();
		}
		range_values.iter().map(ToString::to_string).collect::<Vec<_>>().join(",")
	}

	fn get_joined_string(self, s: &String) -> String {
		if is_selected_all(s) {
			return ALL_SELECTED_TEXT.to_string();
		}
		s.clone()
	}
}

#[derive(Serialize, Deserialize)]
// Tells `serde` to use the "version" field for enum tagging.
#[serde(tag = "version")]
enum VersionedBenchmarkPallet {
	#[serde(rename = "1")]
	V1(BenchmarkPallet),
}

impl VersionedBenchmarkPallet {
	/// Returns the parameters of the benchmarking pallet.
	pub fn parameters(&self) -> BenchmarkPallet {
		match self {
			VersionedBenchmarkPallet::V1(parameters) => parameters.clone(),
		}
	}
}

impl TryFrom<&Path> for VersionedBenchmarkPallet {
	type Error = anyhow::Error;

	fn try_from(bench_file: &Path) -> anyhow::Result<Self> {
		if !bench_file.is_file() {
			return Err(anyhow::anyhow!(format!(
				"Provided invalid benchmarking parameter file: {:?}",
				bench_file.display()
			)));
		}
		let content = fs::read_to_string(bench_file)?;
		toml::from_str(&content)
			.map_err(|e| anyhow::anyhow!("Failed to parse TOML content: {:?}", e.to_string()))
	}
}

impl From<BenchmarkPallet> for VersionedBenchmarkPallet {
	fn from(parameters: BenchmarkPallet) -> Self {
		VersionedBenchmarkPallet::V1(parameters)
	}
}

impl From<VersionedBenchmarkPallet> for BenchmarkPallet {
	fn from(versioned: VersionedBenchmarkPallet) -> Self {
		match versioned {
			VersionedBenchmarkPallet::V1(parameters) => parameters,
		}
	}
}

fn guide_user_to_select_pallets(
	registry: &PalletExtrinsicsRegistry,
	excluded_pallets: &[String],
	cli: &mut impl cli::traits::Cli,
) -> anyhow::Result<Vec<String>> {
	let pallets = pallets(registry, excluded_pallets);
	if pallets.is_empty() {
		return Err(anyhow::anyhow!("No pallets found for the runtime"));
	}

	if cli
		.confirm("Would you like to benchmark all pallets?")
		.initial_value(true)
		.interact()?
	{
		return Ok(vec![ALL_SELECTED.to_string()]);
	}

	let mut prompt = cli.multiselect(r#"🔎 Search for pallets to benchmark"#).filter_mode();
	for pallet in pallets {
		prompt = prompt.item(pallet.clone(), &pallet, "");
	}
	Ok(prompt.interact()?)
}

fn guide_user_to_exclude_pallets(
	registry: &PalletExtrinsicsRegistry,
	cli: &mut impl cli::traits::Cli,
) -> anyhow::Result<Vec<String>> {
	let mut prompt = cli
		.multiselect(r#"🔎 Search for pallets to exclude (Press ENTER to skip)"#)
		.filter_mode()
		.required(false);
	for pallet in pallets(registry, &[]) {
		prompt = prompt.item(pallet.clone(), &pallet, "");
	}
	Ok(prompt.interact()?)
}

fn guide_user_to_exclude_extrinsics(
	registry: &PalletExtrinsicsRegistry,
	pallets: &[String],
	excluded_pallets: &[String],
	cli: &mut impl cli::traits::Cli,
) -> anyhow::Result<Vec<String>> {
	let mut prompt = cli
		.multiselect(r#"🔎 Search for extrinsics to exclude (Press ENTER to skip)"#)
		.filter_mode()
		.required(false);
	for (pallet, extrinsic) in all_pallet_extrinsics(registry, pallets, excluded_pallets, &[]) {
		let value = format_pallet_extrinsic_item((pallet, extrinsic));
		prompt = prompt.item(value.clone(), value, "");
	}
	Ok(prompt.interact()?)
}

fn guide_user_to_select_extrinsics(
	pallets: &[String],
	excluded_pallets: &[String],
	excluded_extrinsics: &[String],
	registry: &PalletExtrinsicsRegistry,
	cli: &mut impl cli::traits::Cli,
) -> anyhow::Result<String> {
	let extrinsics =
		all_pallet_extrinsics(registry, pallets, excluded_pallets, excluded_extrinsics);
	if extrinsics.is_empty() {
		return Err(anyhow::anyhow!(
			"No extrinsics found for the pallets: {:?}",
			pallets.join(", ")
		));
	}

	let pallet_name = if pallet_selected(pallets, excluded_pallets, ALL_SELECTED) {
		ALL_SELECTED_TEXT.to_string()
	} else {
		pallets.join(", ")
	};
	if cli
		.confirm(format!(r#"Would you like to benchmark all extrinsics of "{}"?"#, pallet_name))
		.initial_value(true)
		.interact()?
	{
		return Ok(ALL_SELECTED.to_string());
	}

	let mut prompt = cli
		.multiselect(r#"🔎 Search for extrinsics to benchmark (select with space)"#)
		.filter_mode()
		.required(true);
	for (pallet, extrinsic) in extrinsics {
		prompt = prompt.item(extrinsic.clone(), format!("{pallet}:{extrinsic}"), "");
	}
	Ok(prompt.interact()?.join(","))
}

async fn guide_user_to_select_menu_option(
	cmd: &mut BenchmarkPallet,
	cli: &mut impl cli::traits::Cli,
) -> anyhow::Result<BenchmarkPalletMenuOption> {
	let spinner = spinner();
	let mut prompt = cli.select("Select the parameter to update:");

	let mut index = 0;
	spinner.start("Loading parameters...");
	for param in BenchmarkPalletMenuOption::iter() {
		if param.is_disabled(cmd)? {
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

fn guide_user_to_update_bench_file_path(
	cmd: &mut BenchmarkPallet,
	cli: &mut impl cli::traits::Cli,
	params_updated: bool,
) -> anyhow::Result<Option<PathBuf>> {
	if let Some(ref bench_file) = cmd.bench_file {
		if params_updated &&
			(cmd.skip_confirm ||
				cli.confirm(format!(
					"Do you want to overwrite {:?} with the updated parameters?",
					bench_file.display()
				))
				.initial_value(true)
				.interact()?)
		{
			return Ok(Some(bench_file.clone()));
		}
	} else if cmd.skip_confirm {
		let bench_file = PathBuf::from(DEFAULT_BENCH_FILE);
		if bench_file.extension() != Some(OsStr::new("toml")) {
			return Err(anyhow::anyhow!("Invalid file extension. Expected .toml"));
		}
		cmd.bench_file = Some(bench_file.clone());
		return Ok(Some(bench_file));
	} else if cli
		.confirm(format!(
			"Do you want to save the parameters to {:?}?\n{}.",
			DEFAULT_BENCH_FILE,
			console::style("This will allow loading parameters from the file by using `-f`").dim()
		))
		.initial_value(true)
		.interact()?
	{
		let input = cli
			.input("Provide the output path for benchmark parameter values")
			.required(true)
			.placeholder(DEFAULT_BENCH_FILE)
			.default_input(DEFAULT_BENCH_FILE)
			.interact()?;
		let bench_file = PathBuf::from(input);
		if bench_file.extension() != Some(OsStr::new("toml")) {
			return Err(anyhow::anyhow!("Invalid file extension. Expected .toml"));
		}
		cmd.bench_file = Some(bench_file.clone());
		return Ok(Some(bench_file));
	};
	Ok(None)
}

fn is_selected_all(s: &String) -> bool {
	s == &ALL_SELECTED.to_string() || s.is_empty()
}

// Referenced from `substrate/utils/frame/benchmarking-cli/src/pallet/command.rs`.
fn pallet_selected(pallets: &[String], excluded_pallets: &[String], pallet: &str) -> bool {
	let included = pallets.is_empty() ||
		pallets.iter().any(|p| p == pallet) ||
		pallets.iter().any(|p| p == ALL_SELECTED) ||
		pallets.iter().any(|p| p == "all");

	let excluded = excluded_pallets.iter().any(|p| p == pallet);

	included && !excluded
}

fn pallets(registry: &PalletExtrinsicsRegistry, excluded_pallets: &[String]) -> Vec<String> {
	registry
		.keys()
		.filter(|s| !excluded_pallets.contains(&s.to_string()))
		.map(String::from)
		.collect()
}

fn all_pallet_extrinsics(
	registry: &PalletExtrinsicsRegistry,
	pallets: &[String],
	excluded_pallets: &[String],
	excluded_extrinsics: &[String],
) -> Vec<PalletExtrinsicItem> {
	registry
		.iter()
		.filter(|(pallet_name, _)| pallet_selected(pallets, excluded_pallets, pallet_name))
		.flat_map(|(pallet_name, extrinsics)| {
			extrinsics
				.iter()
				.filter(move |extrinsic| !excluded_extrinsics.contains(extrinsic))
				.map(move |extrinsic| (pallet_name.clone(), extrinsic.clone()))
		})
		.collect()
}

// Add a more relaxed parsing for pallet names by allowing pallet directory names with `-` to be
// used like crate names with `_`
fn parse_pallet_name(pallet: &str) -> Result<String, String> {
	Ok(pallet.replace("-", "_"))
}

// Get relative path. Returns absolute path if the path is not relative.
fn get_relative_path(path: &Path) -> PathBuf {
	let cwd = current_dir().unwrap_or(PathBuf::from("./"));
	get_relative_or_absolute_path(cwd.as_path(), path)
}

fn get_current_directory() -> PathBuf {
	current_dir().unwrap_or(PathBuf::from("./"))
}

fn format_pallet_extrinsic_item((pallet, extrinsic): (String, String)) -> String {
	format!("{pallet}:{extrinsic}")
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::{
		cli::MockCli,
		common::{
			bench::source_omni_bencher_binary,
			runtime::{Feature::Benchmark, get_mock_runtime},
		},
	};
	use anyhow::Ok;
	use pop_common::{Profile, helpers::with_current_dir_async};
	use std::fs::{self, File};
	use strum::EnumMessage;
	use tempfile::tempdir;

	#[tokio::test]
	async fn list_pallets_works() -> anyhow::Result<()> {
		let mut cli = MockCli::new()
			.expect_intro("Listing available pallets and extrinsics")
			.expect_warning(
				"NOTE: the `pop bench pallet` is not yet battle tested - double check the results.",
			)
			.expect_outro("All pallets and extrinsics listed!");
		BenchmarkPallet {
			list: true,
			runtime: Some(get_mock_runtime(Some(Benchmark))),
			..Default::default()
		}
		.execute(&mut cli)
		.await?;
		cli.verify()
	}

	#[tokio::test]
	async fn benchmark_pallet_without_runtime_benchmarks_feature_fails() -> anyhow::Result<()> {
		let mut cli = MockCli::new();
		cli = expect_pallet_benchmarking_intro(cli);
		cli = cli.expect_outro_cancel(
	        "Failed to run benchmarking: Invalid input: Did not find the benchmarking runtime api. This could mean that you either did not build the node correctly with the `--features runtime-benchmarks` flag, or the chain spec that you are using was not created by a node that was compiled with the flag"
		);

		BenchmarkPallet {
			runtime: Some(get_mock_runtime(None)),
			pallets: vec!["pallet_timestamp".to_string()],
			extrinsic: Some(ALL_SELECTED.to_string()),
			skip_parameters: true,
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
		cli = cli.expect_outro_cancel("Failed to run benchmarking: Invalid input: No benchmarks found which match your input. Try `--list --all` to list all available benchmarks. Make sure pallet is in `define_benchmarks!`");

		BenchmarkPallet {
			runtime: Some(get_mock_runtime(Some(Benchmark))),
			pallets: vec!["unknown_pallet".to_string()],
			extrinsic: Some(ALL_SELECTED.to_string()),
			skip_parameters: true,
			genesis_builder: Some(GenesisBuilderPolicy::None),
			..Default::default()
		}
		.execute(&mut cli)
		.await?;
		cli.verify()
	}

	#[tokio::test]
	async fn guide_user_to_select_pallet_works() -> anyhow::Result<()> {
		let temp_cache = tempdir()?;
		let registry = get_registry(temp_cache.path()).await?;
		let pallet_items: Vec<(String, String)> = pallets(&registry, &[])
			.into_iter()
			.map(|pallet| (pallet, Default::default()))
			.collect();
		let prompt = "Would you like to benchmark all pallets?";

		// Select all pallets.
		let mut cli = MockCli::new().expect_confirm(prompt, true);
		assert_eq!(
			guide_user_to_select_pallets(&registry, &[], &mut cli)?,
			vec![ALL_SELECTED.to_string()]
		);
		cli.verify()?;

		// Not exclude pallets.
		cli = MockCli::new().expect_confirm(prompt, false).expect_multiselect(
			r#"🔎 Search for pallets to benchmark"#,
			None,
			true,
			Some(pallet_items.clone()),
			Some(true),
		);
		guide_user_to_select_pallets(&registry, &[], &mut cli)?;
		cli.verify()?;

		// Exclude pallets
		cli = MockCli::new().expect_confirm(prompt, false).expect_multiselect(
			r#"🔎 Search for pallets to benchmark"#,
			None,
			true,
			Some(pallet_items.into_iter().filter(|(p, _)| p != "pallet_timestamp").collect()),
			Some(true),
		);
		guide_user_to_select_pallets(&registry, &["pallet_timestamp".to_string()], &mut cli)?;
		cli.verify()
	}

	#[tokio::test]
	async fn guide_user_to_exclude_pallets_works() -> anyhow::Result<()> {
		let temp_cache = tempdir()?;
		let registry = get_registry(temp_cache.path()).await?;
		let pallet_items = pallets(&registry, &[])
			.into_iter()
			.map(|pallet| (pallet, Default::default()))
			.collect();
		let mut cli = MockCli::new().expect_multiselect(
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
		let temp_cache = tempdir()?;
		let registry = get_registry(temp_cache.path()).await?;

		let mut cli = MockCli::new().expect_confirm(
			r#"Would you like to benchmark all extrinsics of "pallet_timestamp"?"#,
			true,
		);
		assert_eq!(
			guide_user_to_select_extrinsics(
				&["pallet_timestamp".to_string()],
				&[],
				&[],
				&registry,
				&mut cli
			)?,
			ALL_SELECTED.to_string()
		);

		let extrinsic_items =
			all_pallet_extrinsics(&registry, &["pallet_timestamp".to_string()], &[], &[]);
		let mut cli = MockCli::new()
			.expect_confirm(
				r#"Would you like to benchmark all extrinsics of "pallet_timestamp"?"#,
				false,
			)
			.expect_multiselect(
				r#"🔎 Search for extrinsics to benchmark (select with space)"#,
				Some(true),
				true,
				Some(
					extrinsic_items
						.iter()
						.map(|(pallet, extrinsic)| {
							(format!("{}:{}", pallet, extrinsic), String::default())
						})
						.collect(),
				),
				Some(true),
			);
		guide_user_to_select_extrinsics(
			&["pallet_timestamp".to_string()],
			&[],
			&[],
			&registry,
			&mut cli,
		)?;
		cli.verify()
	}

	#[tokio::test]
	async fn guide_user_to_select_menu_option_works() -> anyhow::Result<()> {
		let mut cmd = BenchmarkPallet {
			skip_confirm: false,
			runtime: Some(get_mock_runtime(Some(Benchmark))),
			runtime_binary: Some(get_mock_runtime(Some(Benchmark))),
			pallets: vec![ALL_SELECTED.to_string()],
			extrinsic: Some(ALL_SELECTED.to_string()),
			..Default::default()
		};

		let mut cli = expect_parameter_menu(MockCli::new(), &cmd, 0)?;
		guide_user_to_select_menu_option(&mut cmd, &mut cli).await?;
		cli.verify()
	}

	#[test]
	fn guide_user_to_update_bench_file_path_works() -> anyhow::Result<()> {
		let temp_dir = tempdir()?;
		let file_path = temp_dir.path().join(DEFAULT_BENCH_FILE);
		let invalid_file_path = temp_dir.path().join("invalid_file.txt");
		let file_path_str = file_path.to_str().unwrap().to_string();
		let prompt = format!(
			"Do you want to save the parameters to {:?}?\n{}.",
			DEFAULT_BENCH_FILE,
			console::style("This will allow loading parameters from the file by using `-f`").dim()
		);

		// No bench file path provided.
		let mut cli = MockCli::new().expect_confirm(&prompt, true).expect_input(
			"Provide the output path for benchmark parameter values",
			file_path_str.clone(),
		);
		assert_eq!(
			guide_user_to_update_bench_file_path(&mut BenchmarkPallet::default(), &mut cli, true)?,
			Some(file_path.clone())
		);
		cli.verify()?;

		// Reject to save the updated parameters.
		let mut cli = MockCli::new().expect_confirm(&prompt, false);
		assert_eq!(
			guide_user_to_update_bench_file_path(&mut BenchmarkPallet::default(), &mut cli, true)?,
			None
		);
		cli.verify()?;

		// Invalid file extension.
		let mut cli = MockCli::new().expect_confirm(&prompt, true).expect_input(
			"Provide the output path for benchmark parameter values",
			invalid_file_path.to_str().unwrap().to_string(),
		);
		assert_eq!(
			guide_user_to_update_bench_file_path(&mut BenchmarkPallet::default(), &mut cli, true)
				.err()
				.unwrap()
				.to_string(),
			"Invalid file extension. Expected .toml"
		);
		cli.verify()?;

		// Provide bench file path but reject to overwrite.
		let mut cmd = BenchmarkPallet { bench_file: Some(file_path.clone()), ..Default::default() };
		let mut cli = MockCli::new().expect_confirm(
			format!("Do you want to overwrite {:?} with the updated parameters?", file_path_str),
			false,
		);
		assert_eq!(guide_user_to_update_bench_file_path(&mut cmd, &mut cli, true)?, None);
		cli.verify()?;

		// Provide bench file path.
		let mut cli = MockCli::new().expect_confirm(
			format!(
				"Do you want to overwrite {:?} with the updated parameters?",
				file_path_str.clone()
			),
			true,
		);
		assert_eq!(
			guide_user_to_update_bench_file_path(&mut cmd, &mut cli, true)?,
			Some(file_path)
		);
		cli.verify()?;

		Ok(())
	}

	#[tokio::test]
	async fn menu_option_is_disabled_works() -> anyhow::Result<()> {
		use BenchmarkPalletMenuOption::*;
		let cmd = BenchmarkPallet {
			runtime_binary: Some(get_mock_runtime(None)),
			pallets: vec!["pallet_timestamp".to_string(), "pallet_sudo".to_string()],
			extrinsic: Some(ALL_SELECTED.to_string()),
			genesis_builder: Some(GenesisBuilderPolicy::None),
			..Default::default()
		};
		assert!(!GenesisBuilder.is_disabled(&cmd)?);
		assert!(GenesisBuilderPreset.is_disabled(&cmd)?);
		assert!(WeightFileTemplate.is_disabled(&cmd)?);
		Ok(())
	}

	#[test]
	fn menu_option_read_command_works() -> anyhow::Result<()> {
		use BenchmarkPalletMenuOption::*;
		let cmd = BenchmarkPallet {
			runtime: Some(get_mock_runtime(None)),
			pallets: vec![ALL_SELECTED.to_string()],
			extrinsic: Some(ALL_SELECTED.to_string()),
			genesis_builder: Some(GenesisBuilderPolicy::Runtime),
			exclude_extrinsics: vec![
				String::from("pallet_timestamp:on_finalize"),
				String::from("pallet_sudo:sudo"),
			],
			..Default::default()
		};
		[
			(Pallets, ALL_SELECTED_TEXT),
			(Extrinsics, ALL_SELECTED_TEXT),
			(ExcludedPallets, ARGUMENT_NO_VALUE),
			(ExcludedExtrinsics, "pallet_timestamp:on_finalize,pallet_sudo:sudo"),
			(Runtime, get_mock_runtime(None).to_str().unwrap()),
			(GenesisBuilder, &GenesisBuilderPolicy::Runtime.to_string()),
			(GenesisBuilderPreset, "development"),
			(Steps, "50"),
			(Repeat, "20"),
			(High, ARGUMENT_NO_VALUE),
			(Low, ARGUMENT_NO_VALUE),
			(MapSize, "1000000"),
			(DatabaseCacheSize, "1024"),
			(AdditionalTrieLayer, "2"),
			(NoMedianSlope, "false"),
			(NoMinSquare, "false"),
			(NoStorageInfo, "false"),
			(WeightFileTemplate, ARGUMENT_NO_VALUE),
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
		for (option, _) in options {
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
				format!(
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
		let runtime_path = get_mock_runtime(Some(Benchmark));
		check_omni_bencher_and_prompt(&mut cli, &spinner(), true).await?;
		let cmd = BenchmarkPallet { runtime_binary: Some(runtime_path), ..Default::default() };
		let mut registry = PalletExtrinsicsRegistry::default();

		// Load pallet registry if the cached registry is empty.
		cmd.ensure_pallet_registry(&mut cli, &mut registry).await?;
		let mut pallet_names: Vec<String> = registry.keys().map(String::from).collect();
		pallet_names.sort();
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
		let mock_registry = get_mock_registry();
		registry = mock_registry.clone();
		cmd.ensure_pallet_registry(&mut cli, &mut registry).await?;
		assert_eq!(registry, mock_registry);

		Ok(())
	}

	#[test]
	fn get_runtime_works() -> anyhow::Result<()> {
		assert_eq!(
			BenchmarkPallet { runtime: Some(get_mock_runtime(None)), ..Default::default() }
				.runtime()?,
			&get_mock_runtime(None)
		);
		assert!(matches!(BenchmarkPallet::default().runtime(), Err(message)
			if message.to_string().contains("No runtime found")
		));
		Ok(())
	}

	#[test]
	fn get_runtime_binary_works() -> anyhow::Result<()> {
		assert_eq!(
			BenchmarkPallet { runtime_binary: Some(get_mock_runtime(None)), ..Default::default() }
				.runtime_binary()?,
			&get_mock_runtime(None)
		);
		assert!(matches!(BenchmarkPallet::default().runtime_binary(), Err(message)
			if message.to_string().contains("No runtime binary found")
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

	#[test]
	fn versioned_benchmark_pallet_serialization_works() {
		let benchmark_pallet = BenchmarkPallet::default();
		let versioned = VersionedBenchmarkPallet::V1(benchmark_pallet.clone());
		let toml_str = toml::to_string(&versioned).expect("Failed to serialize");
		assert!(toml_str.contains("version = \"1\""));
		let deserialized: VersionedBenchmarkPallet =
			toml::from_str(&toml_str).expect("Failed to deserialize");
		assert_eq!(BenchmarkPallet::from(deserialized), benchmark_pallet);
	}

	#[test]
	fn versioned_benchmark_pallet_parameters_works() {
		let benchmark_pallet = BenchmarkPallet::default();
		let versioned = VersionedBenchmarkPallet::V1(benchmark_pallet.clone());
		assert_eq!(versioned.parameters(), benchmark_pallet);
	}

	#[test]
	fn versioned_benchmark_pallet_try_from_valid_file() -> anyhow::Result<()> {
		let temp_dir = tempdir()?;
		let file_path = temp_dir.path().join(DEFAULT_BENCH_FILE);
		let benchmark_pallet = BenchmarkPallet::default();
		let versioned = VersionedBenchmarkPallet::from(benchmark_pallet);

		let toml_str = toml::to_string(&versioned)?;
		fs::write(&file_path, toml_str)?;

		let parsed = VersionedBenchmarkPallet::try_from(file_path.as_path())?;
		assert!(matches!(parsed, VersionedBenchmarkPallet::V1(_)));
		Ok(())
	}

	#[test]
	fn versioned_benchmark_pallet_try_from_invalid_file() -> anyhow::Result<()> {
		let temp_dir = tempdir()?;
		let file_path = temp_dir.path().join("invalid.toml");

		// Provide missing file.
		assert_eq!(
			VersionedBenchmarkPallet::try_from(file_path.as_path())
				.err()
				.unwrap()
				.to_string(),
			format!(r#"Provided invalid benchmarking parameter file: "{}""#, file_path.display())
		);
		// Write invalid TOML content
		fs::write(&file_path, "invalid toml content").expect("Failed to write to file");
		assert_eq!(
			VersionedBenchmarkPallet::try_from(file_path.as_path())
				.err()
				.unwrap()
				.to_string(),
			r#"Failed to parse TOML content: "expected an equals, found an identifier at line 1 column 9""#
		);
		Ok(())
	}

	#[tokio::test]
	async fn update_pallets_works() -> anyhow::Result<()> {
		// Load pallet registry if the registry is empty.
		let mut cli =
			MockCli::new().expect_confirm("Would you like to benchmark all pallets?", true);
		check_omni_bencher_and_prompt(&mut cli, &spinner(), true).await?;
		let mut registry = PalletExtrinsicsRegistry::default();
		BenchmarkPallet {
			runtime_binary: Some(get_mock_runtime(Some(Benchmark))),
			..Default::default()
		}
		.update_pallets(&mut cli, &mut registry)
		.await?;
		assert!(!registry.is_empty());

		let pallets: Vec<String> = pallets(&registry, &[]).into_iter().collect();
		let pallet_items: Vec<(String, String)> = pallets
			.clone()
			.iter()
			.map(|pallet| (pallet.clone(), String::default()))
			.collect();
		for (select_all, mut cmd, expected_pallets) in [
			// Select all pallets
			(
				true,
				BenchmarkPallet { pallets: vec![ALL_SELECTED.to_string()], ..Default::default() },
				vec![ALL_SELECTED.to_string()],
			),
			(
				false,
				BenchmarkPallet { pallets: pallets.clone(), ..Default::default() },
				pallets.clone(),
			),
		] {
			let mut cli = MockCli::new()
				.expect_confirm("Would you like to benchmark all pallets?", select_all);
			if !select_all {
				cli = cli.expect_multiselect(
					r#"🔎 Search for pallets to benchmark"#,
					None,
					true,
					Some(pallet_items.clone()),
					Some(true),
				);
			}
			cmd.update_pallets(&mut cli, &mut registry).await?;
			assert_eq!(cmd.pallets, expected_pallets);
			cli.verify()?;
		}

		Ok(())
	}

	#[tokio::test]
	async fn update_extrinsic_works() -> anyhow::Result<()> {
		let pallet = "pallet_timestamp";
		check_omni_bencher_and_prompt(&mut MockCli::new(), &spinner(), true).await?;
		// Load pallet registry if the registry is empty.
		let mut registry = PalletExtrinsicsRegistry::default();
		BenchmarkPallet {
			runtime_binary: Some(get_mock_runtime(Some(Benchmark))),
			pallets: vec![ALL_SELECTED.to_string()],
			..Default::default()
		}
		.update_extrinsics(&mut MockCli::new(), &mut registry)
		.await?;
		assert!(!registry.is_empty());

		// If `pallet` is "*", select all extrinsics.
		let mut cmd =
			BenchmarkPallet { pallets: vec![ALL_SELECTED.to_string()], ..Default::default() };
		cmd.update_extrinsics(&mut MockCli::new(), &mut registry).await?;
		assert_eq!(cmd.extrinsic, Some(ALL_SELECTED.to_string()));

		// Select all extrinsics of the `pallet`.
		let prompt = format!(r#"Would you like to benchmark all extrinsics of {:?}?"#, pallet);
		let mut cli = MockCli::new().expect_confirm(prompt, true);
		let mut cmd = BenchmarkPallet { pallets: vec![pallet.to_string()], ..Default::default() };
		cmd.update_extrinsics(&mut cli, &mut registry).await?;
		assert_eq!(cmd.extrinsic, Some(ALL_SELECTED.to_string()));
		cli.verify()?;
		Ok(())
	}

	#[tokio::test]
	async fn update_excluded_pallets_works() -> anyhow::Result<()> {
		let temp_cache = tempdir()?;

		let registry = get_registry(temp_cache.path()).await?;
		let pallet_items = pallets(&registry, &[])
			.into_iter()
			.map(|pallet| (pallet, Default::default()))
			.collect();
		let mut cli = MockCli::new().expect_multiselect(
			r#"🔎 Search for pallets to exclude (Press ENTER to skip)"#,
			Some(false),
			true,
			Some(pallet_items),
			Some(true),
		);

		// Load pallet registry if the registry is empty.
		let mut cmd = BenchmarkPallet {
			runtime_binary: Some(get_mock_runtime(Some(Benchmark))),
			..Default::default()
		};
		check_omni_bencher_and_prompt(&mut cli, &spinner(), true).await?;
		let mut registry = PalletExtrinsicsRegistry::default();
		cmd.update_excluded_pallets(&mut cli, &mut registry).await?;
		assert!(!registry.is_empty());

		// Update the `exclude_pallets`.
		let excluded_pallets = registry.keys().cloned().collect::<Vec<_>>();
		assert_eq!(cmd.exclude_pallets, excluded_pallets);

		Ok(())
	}

	#[tokio::test]
	async fn update_excluded_extrinsics_works() -> anyhow::Result<()> {
		let temp_cache = tempdir()?;
		let registry = get_registry(temp_cache.path()).await?;
		let pallet_extrinsics = all_pallet_extrinsics(&registry, &[], &[], &[])
			.into_iter()
			.map(|pallet| (format_pallet_extrinsic_item(pallet), Default::default()))
			.collect();
		let mut cli = MockCli::new().expect_multiselect(
			r#"🔎 Search for extrinsics to exclude (Press ENTER to skip)"#,
			Some(false),
			true,
			Some(pallet_extrinsics),
			Some(true),
		);

		// Load pallet registry if the registry is empty.
		let mut cmd = BenchmarkPallet {
			runtime_binary: Some(get_mock_runtime(Some(Benchmark))),
			..Default::default()
		};
		check_omni_bencher_and_prompt(&mut cli, &spinner(), true).await?;
		let mut registry = PalletExtrinsicsRegistry::default();
		cmd.update_excluded_extrinsics(&mut cli, &mut registry).await?;
		assert!(!registry.is_empty());

		// Update the `exclude_extrinsics`.
		let excluded_extrinsics: Vec<String> = all_pallet_extrinsics(&registry, &[], &[], &[])
			.into_iter()
			.map(format_pallet_extrinsic_item)
			.collect();
		assert_eq!(cmd.exclude_extrinsics, excluded_extrinsics);

		Ok(())
	}

	#[tokio::test]
	async fn update_runtime_path_works() -> anyhow::Result<()> {
		let temp_dir = tempdir()?;
		let temp_path = temp_dir.path().to_path_buf();

		// Create workspace structure
		let workspace_toml = temp_path.join("Cargo.toml");
		fs::write(
			&workspace_toml,
			r#"[workspace]
members = ["runtime"]
resolver = "2"

[workspace.package]
name = "test-workspace"
"#,
		)?;

		// Create runtime directory with Cargo.toml
		let runtime_dir = temp_path.join("runtime");
		fs::create_dir_all(runtime_dir.join("src"))?;
		fs::write(
			runtime_dir.join("Cargo.toml"),
			r#"[package]
name = "runtime"
version = "0.1.0"

[dependencies]

[features]
runtime-benchmarks = []
"#,
		)?;
		fs::write(runtime_dir.join("src").join("lib.rs"), "fn main() {}")?;

		// Create target directory structure with wasm binary
		fs::create_dir(temp_path.join("target"))?;
		let target_path = Profile::Debug.target_directory(temp_path.as_path());
		fs::create_dir(&target_path)?;
		let wbuild_path = target_path.join("wbuild").join("runtime");
		fs::create_dir_all(&wbuild_path)?;
		let binary_path = wbuild_path.join("runtime.wasm");
		File::create(&binary_path)?;

		// Canonicalize the runtime path before setting up expectations
		let canonicalized_runtime = runtime_dir.canonicalize()?;

		let mut cli = MockCli::new()
			.expect_select(
				"Choose the build profile of the binary that should be used: ".to_string(),
				Some(true),
				true,
				Some(Profile::get_variants()),
				0,
				None,
			)
			.expect_info(format!("Using runtime at {}", canonicalized_runtime.display()));
		let mut cmd = BenchmarkPallet { no_build: true, ..Default::default() };
		let result =
			with_current_dir_async(&temp_path, async || cmd.update_runtime_path(&mut cli).await)
				.await;

		assert!(result.is_ok(), "Failed to update runtime path: {:?}", result);
		assert_eq!(cmd.runtime, Some(PathBuf::from("runtime")));
		cli.verify()?;
		Ok(())
	}

	#[test]
	fn update_template_path_works() -> anyhow::Result<()> {
		let temp_dir = tempdir()?;

		// Provided template path is not an existing file.
		let mut cli = MockCli::new().expect_input(
			"Provide path to the custom template for generated weight files (optional)",
			temp_dir.path().join("template.txt").to_str().unwrap().to_string(),
		);
		assert_eq!(
			BenchmarkPallet::default()
				.update_template_path(&mut cli)
				.err()
				.unwrap()
				.to_string(),
			"Template path does not exist or is a directory"
		);
		cli.verify()?;

		// Provided template path is a directory.
		let mut cli = MockCli::new().expect_input(
			"Provide path to the custom template for generated weight files (optional)",
			temp_dir.path().to_str().unwrap().to_string(),
		);
		assert_eq!(
			BenchmarkPallet::default()
				.update_template_path(&mut cli)
				.err()
				.unwrap()
				.to_string(),
			"Template path does not exist or is a directory"
		);
		cli.verify()?;
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
		item: usize,
	) -> anyhow::Result<MockCli> {
		let mut items: Vec<(String, String)> = vec![];
		let mut index = 0;
		for param in BenchmarkPalletMenuOption::iter() {
			if param.is_disabled(cmd)? {
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

	async fn get_registry(cache_dir: &Path) -> anyhow::Result<PalletExtrinsicsRegistry> {
		let runtime_path = get_mock_runtime(Some(Benchmark));
		let binary_path =
			source_omni_bencher_binary(&mut MockCli::new(), &spinner(), cache_dir, true).await?;
		Ok(load_pallet_extrinsics(&runtime_path, binary_path.as_path()).await?)
	}

	fn get_mock_registry() -> PalletExtrinsicsRegistry {
		PalletExtrinsicsRegistry::from([
			("pallet_timestamp".to_string(), vec!["on_finalize".to_string(), "set".to_string()]),
			("frame_system".to_string(), vec!["set_code".to_string(), "remark".to_string()]),
		])
	}
}
