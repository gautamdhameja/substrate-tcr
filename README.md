# Substrate - Token Curated Registries

An implementation of (a subset of) [Token Curated Registries](https://medium.com/@ilovebagels/token-curated-registries-1-0-61a232f8dac7) (TCR) as a [Parity Substrate](https://www.parity.io/substrate/) runtime. Built using [substrate-node-template](https://github.com/paritytech/substrate/tree/master/node-template) codebase.

The TCR runtime is implemented as a multi-module runtime with following two modules.

1. **tcr.rs** - The main module with all curation and initialization functions. The module implements a simple-TCR as described and implemented [here](https://github.com/gautamdhameja/simple-tcr). The full TCR functionality in not implemented but only basic curation functions are.
2. **token.rs** - Implementation of the modified ERC20 interface to serve as the native token for the TCR module. There are some additional functions implemented for locking and unlocking of tokens.

## Usage

### Step 0

If you are new to [Substrate](https://www.parity.io/substrate/), please go through the [getting started tutorial](https://substrate.readme.io/docs/creating-a-custom-substrate-chain) first. It will give you a sense of how the code is structured in a `substrate-node-template` and how to get it up and running.

This will also ensure that you have Rust and Substrate installed on your system.

### Step 1

Clone this repository. Inside the directory where you have cloned, run the following commands,

* To build the `WASM` runtime for the node (**run this first**),

```
./build.sh
```

* To build the rust code and the node (**run this after `./build.sh` completes**),

```
cargo build --release
```

* To start the node

```
./target/release/substrate-tcr --dev
```

The TCR runtime should be up in the local substrate node running at `localhost:9944`.

### Step 2 (UI)

You can either use the [Substrate-TCR-UI](https://github.com/parity-samples/substrate-tcr-ui) to connect with this runtime or you can try it out using the [Polkadot Apps UI](https://polkadot.js.org/apps/).

To try it with the [Polkadot Apps UI](https://polkadot.js.org/apps/), follow the following steps,

* Once the local node is running, open the following in your browser,

```
https://polkadot.js.org/apps/
```

* Go to the settings page and select `Local Node` in the `remote node/endpoint to connect to` input. Click `Save & Reload`.

For further instructions on using the runtime with the Polkadot Apps UI, please see this [wiki page](https://github.com/parity-samples/substrate-tcr/wiki/How-to-test-the-TCR-runtime-using-Polkadot-Apps-Portal) in this repository.

### Important Note

The Substrate framework, related libraries and APIs are rapidly evolving. In case this module does not work with the latest Substrate build, please submit an issue in this repo.
You can also try porting the runtime module into a freshly cloned `substrate-node-template` codebase.

### Important Note 2

This code is just a sample for learning purposes. It is not audited and reviewed for production use cases. You can expect bugs and security vulnerabilities. Do **not** use it as-is in real applications.
