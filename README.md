# YPIR

This is an implementation of the YPIR scheme for single-server private information retrieval,
introduced in ["YPIR: High-Throughput Single-Server PIR with Silent Preprocessing"](https://eprint.iacr.org/2024/270).
This is joint work with [David Wu](https://www.cs.utexas.edu/~dwu4/).

## Running

To build and run this code:
1. Ensure you are running on Ubuntu, and that AVX-512 is available on the CPU (you can run `lscpu` and look for the `avx512f` flag).
Our benchmarks were collected using the AWS `r6i.16xlarge` instance type, which has all necessary CPU features.
2. Run `sudo apt-get update && sudo apt-get install -y build-essential`.
2. [Install Rust using rustup](https://www.rust-lang.org/tools/install) using `curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh`.
  - Select `1) Proceed with installation (default)` when prompted
  - After installation, configure the current shell as instructed by running `source "$HOME/.cargo/env"`
3. Run `git clone https://github.com/menonsamir/ypir.git` and `cd ypir`.
4. Run `cargo run --release -- 1073741824` to run YPIR on a random database consisting of 1073741824 bits (~134 MB).
The first time you run this command, Cargo will download and install the necessary libraries to build the code (~2 minutes);
later calls will not take as long. Stability warnings can be safely ignored.
See below for details on how to interpret the measurements.

We have tested the above steps on a fresh AWS `r6i.16xlarge` Ubuntu 22.04 instance and confirmed they work.

## WASM Client

The `ypir-wasm/` crate provides a WebAssembly-compatible client that can run in browsers or Node.js. This allows the PIR client (query generation and response decoding) to run entirely in the browser while the server runs natively.

### API

The WASM client exposes three functions:

- **`ypir_client_init(params_json: &str) -> YpirWasmClient`** — Initialize a client from a JSON params string (provided by the server).
- **`ypir_generate_query(client: &mut YpirWasmClient, target_row: usize) -> Vec<u8>`** — Generate an encrypted query for the given row index. Returns serialized bytes containing the packed query and condensed public parameters.
- **`ypir_decode_response(client: &YpirWasmClient, response_bytes: &[u8]) -> Vec<u8>`** — Decrypt a server response into plaintext bytes.

### Native (Rust) integration

For Rust consumers (e.g., a server that needs to process queries from WASM clients), `ypir-wasm` also provides native helper functions (available on non-wasm targets):

- **`params_to_json(params: &Params) -> String`** — Serialize `Params` to the JSON format expected by `ypir_client_init`.
- **`deserialize_query(params: &Params, query_bytes: &[u8]) -> (Vec<u64>, Vec<PolyMatrixNTT>)`** — Deserialize the query bytes produced by `ypir_generate_query` into the packed query and condensed public parameters needed by the server.
- **`serialize_response(response: &[Vec<u8>]) -> Vec<u8>`** — Flatten a server response into the byte format expected by `ypir_decode_response`.

### Query wire format

The query bytes returned by `ypir_generate_query` use a length-prefixed binary format:

```
[4B LE u32] packed_query.len() (number of u64 elements)
[N×8 bytes] packed_query data (u64 little-endian)
[4B LE u32] num_pub_params
For each pub_param:
  [4B LE u32] rows
  [4B LE u32] cols
  [4B LE u32] data.len() (number of u64 elements)
  [M×8 bytes] polynomial matrix data (u64 little-endian)
```

### Building for WASM

```bash
# Install dependencies
rustup target add wasm32-unknown-unknown
cargo install wasm-pack

# Build the WASM package
cd ypir-wasm
wasm-pack build --target web --release

# Run WASM tests (Node.js)
RUSTFLAGS='-C target-feature=+reference-types' wasm-pack test --node --release -- --test wasm_test

# Run native roundtrip tests (requires server feature)
cargo test --release --features server-test -- --nocapture
```

### Protocol flow

A typical integration looks like:

1. **Server** derives params via `params_for_scenario_simplepir(num_rows, total_size)`, serializes with `ypir_wasm::params_to_json(&params)`, and sends the JSON to the client.
2. **Client (WASM)** calls `ypir_client_init(params_json)` then `ypir_generate_query(&mut client, target_row)` and sends the query bytes to the server.
3. **Server** calls `ypir_wasm::deserialize_query(&params, &query_bytes)` to get the packed query and pub params, runs `y_server.perform_online_computation_simplepir(...)`, and sends the flattened response bytes back via `ypir_wasm::serialize_response(&response)`.
4. **Client (WASM)** calls `ypir_decode_response(&client, &response_bytes)` to get the plaintext row data.

### Options
To pass arguments, make sure to run `cargo run --release -- <ARGS>` (the ` -- ` is important).
Passing `--verbose` or setting the environment variable `RUST_LOG=debug`
will enable detailed logging. All PIR results are checked for correctness.
The full command-line parameters are as follows:

```
Usage: cargo run --release -- [OPTIONS] <NUM_ITEMS> [ITEM_SIZE_BITS] [NUM_CLIENTS] [TRIALS] [OUT_REPORT_JSON]

Run the YPIR scheme with the given parameters

Arguments:
  <NUM_ITEMS>        Number of items in the database
  [ITEM_SIZE_BITS]   Size of each item in bits (optional, default 1), values over 8 are unsupported
  [NUM_CLIENTS]      Number of clients (optional, default 1) to perform cross-client batching over
  [TRIALS]           Number of trials (optional, default 5) to run the YPIR scheme and average performance measurements over (a warmup trial is excluded)
  [OUT_REPORT_JSON]  Output report file (optional) where results will be written in JSON

Options:
  -i, --is-simplepir  Verbose mode (optional) if set, run YPIR+SP instead of standard YPIR
  -v, --verbose       Verbose mode (optional) if set, the program will print debug logs to stderr
  -h, --help          Print help
  -V, --version       Print version
```

### Item sizes

Standard YPIR supports item sizes of 1-8 bits. YPIR+SP supports item of size 28672 bits or larger. To run YPIR+SP for a PIR problem for `N` items, where each item is of size `B` bits, and `B < 286721`, compute `N' = N * B / 28672`, and run YPIR+SP on `N'` items of size 28672 bits.

### Interpreting measurements
This is an annotated version of the output, detailing what each measurement means:
```js
{
  "offline": {
    // Bytes uploaded by the client in the offline phase
    "uploadBytes": 0,

    // Bytes downloaded by the client in the offline phase
    "downloadBytes": 0,

    // Server computation time, in milliseconds, in the offline phase.
    // Includes any precomputation that must be performed on the plaintext database.
    "serverTimeMs": 3965,

    // Not used.
    "clientTimeMs": 0,

    // Time spent precomputing just the SimplePIR hint.
    "simplepirPrepTimeMs": 2539,

    // Bytes that the client *would* have to download, in the offline phase,
    // if they were performing SimplePIR (rather than YPIR)
    // using this implementation (SimplePIR* in the paper).
    "simplepirHintBytes": 29360128,

    // Similarly, bytes that the client *would* have to download,
    // in the offline phase DoublePIR (DoublePIR* in the paper).
    "doublepirHintBytes": 14680064
  },
  "online": {
    // Bytes uploaded by a single client in the online phase.
    "uploadBytes": 604160,

    // Bytes downloaded by a single client in the online phase.
    "downloadBytes": 12288,

    // Bytes that the client *would* have to download, in the online phase,
    // if they were performing SimplePIR (SimplePIR* in the paper).
    "simplepirRespBytes": 28672,

    // Bytes that the client *would* have to download, in the online phase,
    // if they were performing DoublePIR (DoublePIR* in the paper).
    "doublepirRespBytes": 12288,

    // Server computation time, in milliseconds, in the online phase.
    // This is the average time over 5 trials, after a warmup trial.
    "serverTimeMs": 402,

    // Time that the client took to generate the query.
    "clientQueryGenTimeMs": 530,

    // Time that the client took to decode the response (may round down to 0ms).
    "clientDecodeTimeMs": 0,

    // Time spent in the first pass of YPIR (the 'SimplePIR' phase)
    "firstPassTimeMs": 9,

    // Time spent in the second pass of YPIR (the 'DoublePIR' phase)
    "secondPassTimeMs": 3,

    // Time spent performing LWE-to-RLWE conversion
    "ringPackingTimeMs": 387,

    // Not used.
    "sqrtNBytes": 8192,

    // The full set of measured server computation times.
    "allServerTimesMs": [
      401,
      403,
      402,
      401,
      401
    ],
    // The standard deviation of the measured server computation times.
    "stdDevServerTimeMs": 0.8
  }
}
```

### Acknowledgements

YPIR is based on [DoublePIR](https://eprint.iacr.org/2022/949), and this implementation
uses matrix-vector multiplication routines based on the ones in [ahenzinger/simplepir](https://github.com/ahenzinger/simplepir).
We also use the [menonsamir/spiral-rs](https://github.com/menonsamir/spiral-rs) library for Spiral to handle RLWE ciphertexts.

### Citing

Please cite this work as:

```
@inproceedings{MW24,
  author    = {Samir Jordan Menon and David J. Wu},
  title     = {{YPIR}: High-Throughput Single-Server {PIR} with Silent Preprocessing},
  booktitle = {{USENIX} Security Symposium},
  year      = {2024}
}
```
