pub mod config;
pub mod irgen;
pub mod output;
pub mod parser;

#[cfg(target_arch = "wasm32")]
use wasm_bindgen::prelude::*;

#[cfg(not(target_arch = "wasm32"))]
use codespan_reporting::diagnostic::{Diagnostic, Severity};
#[cfg(target_arch = "wasm32")]
use codespan_reporting::{
	diagnostic::Severity,
	files::SimpleFile,
	term::{self, termcolor::Buffer},
};

#[cfg(not(target_arch = "wasm32"))]
use std::path::PathBuf;

#[derive(Debug)]
#[cfg(not(target_arch = "wasm32"))]
pub struct Output {
	pub path: PathBuf,
	pub code: String,
	pub defs: Option<String>,
}

#[cfg(not(target_arch = "wasm32"))]
fn make_output(path: PathBuf, code: String, defs: Option<String>) -> Output {
	Output { path, code, defs }
}

#[cfg(target_arch = "wasm32")]
#[derive(Debug, Clone)]
#[wasm_bindgen(getter_with_clone)]
pub struct Output {
	pub code: String,
	pub defs: Option<String>,
}

#[derive(Debug)]
#[cfg(not(target_arch = "wasm32"))]
pub struct Code {
	pub server: Output,
	pub client: Output,
	pub server_shards: Vec<Output>,
	pub client_shards: Vec<Output>,
	pub types: Option<Output>,
	pub tooling: Option<Output>,
}

#[cfg(target_arch = "wasm32")]
#[derive(Debug, Clone)]
#[wasm_bindgen(getter_with_clone)]
pub struct Code {
	pub server: Output,
	pub client: Output,
	pub types: Option<Output>,
	pub tooling: Option<Output>,
}

#[derive(Debug)]
#[cfg(not(target_arch = "wasm32"))]
pub struct Return {
	pub code: Option<Code>,
	pub diagnostics: Vec<Diagnostic<()>>,
}

#[cfg(target_arch = "wasm32")]
#[derive(Debug)]
#[wasm_bindgen(getter_with_clone)]
pub struct Return {
	pub code: Option<Code>,
	pub diagnostics: String,
}

#[cfg(not(target_arch = "wasm32"))]
pub fn run(input: &str, no_warnings: bool) -> Return {
	let (config, reports) = parser::parse(input);
	let diagnostics = reports
		.into_iter()
		.map(|report| report.to_diagnostic(no_warnings))
		.collect::<Vec<Diagnostic<()>>>();

	if !diagnostics.iter().any(|diag| diag.severity == Severity::Error)
		&& let Some(config) = config
	{
		let split_at = config.max_events_per_file;
		let all_entries = output::luau::split::collect_entries(&config);
		let needs_split = split_at.map_or(false, |max| all_entries.len() > max);

		if needs_split {
			let max = split_at.unwrap();
			let chunks: Vec<Vec<_>> = all_entries.chunks(max).map(|c| c.to_vec()).collect();
			let num_shards = chunks.len();

			let mut server_shards: Vec<Output> = Vec::with_capacity(num_shards);
			let mut client_shards: Vec<Output> = Vec::with_capacity(num_shards);

			for (i, chunk) in chunks.into_iter().enumerate() {
				let shard_cfg = output::luau::split::make_shard_config(&config, chunk, i);
				let s_path: PathBuf = output::luau::split::shard_path(config.server_output, i).into();
				let c_path: PathBuf = output::luau::split::shard_path(config.client_output, i).into();
				server_shards.push(make_output(
					s_path,
					output::luau::server::code(&shard_cfg, true),
					output::typescript::server::code(&shard_cfg),
				));
				client_shards.push(make_output(
					c_path,
					output::luau::client::code(&shard_cfg, true),
					output::typescript::client::code(&shard_cfg),
				));
			}

			return Return {
				code: Some(Code {
					server: make_output(
						config.server_output.into(),
						output::luau::split::aggregator_server_code(&config, num_shards),
						output::typescript::server::code(&config),
					),
					client: make_output(
						config.client_output.into(),
						output::luau::split::aggregator_client_code(&config, num_shards),
						output::typescript::client::code(&config),
					),
					server_shards,
					client_shards,
					types: config.types_output.map(|types_output| make_output(
						types_output.into(),
						output::luau::types::code(&config),
						output::typescript::types::code(&config),
					)),
					tooling: output::tooling::code(&config),
				}),
				diagnostics,
			};
		}

		return Return {
			code: Some(Code {
				server: make_output(
					config.server_output.into(),
					output::luau::server::code(&config, false),
					output::typescript::server::code(&config),
				),
				client: make_output(
					config.client_output.into(),
					output::luau::client::code(&config, false),
					output::typescript::client::code(&config),
				),
				server_shards: Vec::new(),
				client_shards: Vec::new(),
				types: config.types_output.map(|types_output| make_output(
					types_output.into(),
					output::luau::types::code(&config),
					output::typescript::types::code(&config),
				)),
				tooling: output::tooling::code(&config),
			}),
			diagnostics,
		};
	}

	Return {
		code: None,
		diagnostics,
	}
}

#[cfg(target_arch = "wasm32")]
#[wasm_bindgen]
pub fn run(input: &str, no_warnings: bool, ansi_output_supported: bool) -> Return {
	let (config, reports) = parser::parse(input);

	let mut writer = if ansi_output_supported {
		Buffer::ansi()
	} else {
		Buffer::no_color()
	};

	let file = SimpleFile::new("input.zap", input);
	let term_config = term::Config::default();

	let mut no_errors = true;

	for report in reports {
		let diagnostic = report.to_diagnostic(no_warnings);

		if diagnostic.severity == Severity::Error {
			no_errors = false;
		}

		term::emit(&mut writer, &term_config, &file, &diagnostic).unwrap();
	}

	let diagnostics = String::from_utf8(writer.into_inner()).unwrap();

	if no_errors {
		if let Some(config) = config {
			return Return {
				code: Some(Code {
					server: Output {
						code: output::luau::server::code(&config, false),
						defs: output::typescript::server::code(&config),
					},
					client: Output {
						code: output::luau::client::code(&config, false),
						defs: output::typescript::client::code(&config),
					},
					types: config.types_output.map(|_| Output {
						code: output::luau::types::code(&config),
						defs: output::typescript::types::code(&config),
					}),
					tooling: output::tooling::code(&config),
				}),
				diagnostics,
			};
		}
	}

	Return {
		code: None,
		diagnostics,
	}
}
