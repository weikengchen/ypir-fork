use wasm_bindgen::prelude::*;
use wasm_bindgen_test::*;

// No configuration = defaults to node when using `wasm-pack test --node`

/// Small params JSON for testing (2048 rows, 1 instance).
/// Captured from params_for_scenario_simplepir(2048, 2048 * 14).
fn small_params_json() -> &'static str {
    r#"{"db_item_size":3584,"instances":1,"moduli":["268369921","249561089"],"n":1,"noise_width":16.042421,"nu_1":0,"nu_2":1,"p":16384,"poly_len":2048,"q2_bits":28,"t_conv":4,"t_exp_left":3,"t_exp_right":2,"t_gsw":3,"version":0}"#
}

#[wasm_bindgen]
extern "C" {
    #[wasm_bindgen(js_namespace = performance)]
    fn now() -> f64;
}

#[wasm_bindgen_test]
fn test_wasm_client_init() {
    let _client = ypir_wasm::ypir_client_init(small_params_json());
}

#[wasm_bindgen_test]
fn test_wasm_query_gen() {
    let mut client = ypir_wasm::ypir_client_init(small_params_json());
    let query_bytes = ypir_wasm::ypir_generate_query(&mut client, 42);
    assert!(!query_bytes.is_empty());
}

#[wasm_bindgen_test]
fn test_wasm_decode_empty_response() {
    let client = ypir_wasm::ypir_client_init(small_params_json());
    let decoded = ypir_wasm::ypir_decode_response(&client, &[]);
    assert!(decoded.is_empty());
}

/// Benchmark test — prints timing via console_log! (visible when run with
/// WASM_BINDGEN_TEST_TIMEOUT=60 wasm-pack test --node -- --test wasm_test).
/// Set BENCH_WASM_PRINT=1 to force-show results via throw_str (test will "fail").
#[wasm_bindgen_test]
fn bench_wasm_client_ops() {
    let t0 = now();
    let mut client = ypir_wasm::ypir_client_init(small_params_json());
    let t1 = now();

    let query_bytes = ypir_wasm::ypir_generate_query(&mut client, 42);
    let t2 = now();

    let _ = ypir_wasm::ypir_decode_response(&client, &[]);
    let t3 = now();

    let msg = format!(
        "WASM benchmark:\n  client init:      {:.2} ms\n  query gen:        {:.2} ms\n  decode (empty):   {:.2} ms\n  query size:       {} bytes",
        t1 - t0,
        t2 - t1,
        t3 - t2,
        query_bytes.len()
    );
    console_log!("{}", msg);
}
