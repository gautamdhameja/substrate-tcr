
				use substrate_wasm_builder::build_project_with_default_rustflags;

				fn main() {
					build_project_with_default_rustflags(
						"/home/gautam/code/substrate-tcr/target/rls/debug/build/substrate-tcr-runtime-b8ea844d60642f11/out/wasm_binary.rs",
						"/home/gautam/code/substrate-tcr/runtime/Cargo.toml",
						"-Clink-arg=--export=__heap_base",
					)
				}
			