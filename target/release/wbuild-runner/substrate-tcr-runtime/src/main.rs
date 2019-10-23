
				use substrate_wasm_builder::build_project_with_default_rustflags;

				fn main() {
					build_project_with_default_rustflags(
						"/home/gautam/code/substrate-tcr/target/release/build/substrate-tcr-runtime-ebeb9cc4cc3401bb/out/wasm_binary.rs",
						"/home/gautam/code/substrate-tcr/runtime/Cargo.toml",
						"-Clink-arg=--export=__heap_base",
					)
				}
			