#![cfg(not(target_arch = "wasm32"))]

use std::time::Instant;

use spiral_rs::aligned_memory::AlignedMemory64;
use spiral_rs::poly::PolyMatrixNTT;

use ypir::measurement::Measurement;
use ypir::params::params_for_scenario_simplepir;
use ypir::server::{DbRowsPadded, YServer};
use ypir::util::ToU64;

use ypir_wasm::params_to_json;

#[test]
fn test_roundtrip_small() {
    // 1. Create params for a small DB: 2048 rows, 1 instance
    let params = params_for_scenario_simplepir(2048, 2048 * 14);

    let db_rows = 1 << (params.db_dim_1 + params.poly_len_log2);
    let db_cols = params.instances * params.poly_len;

    // 2. Create server with known data
    let data: Vec<u16> = (0..(db_rows * db_cols) as u64)
        .map(|i| (i % params.pt_modulus) as u16)
        .collect();
    let y_server = YServer::<u16>::new(&params, data.iter().copied(), true, false, true);
    let offline_vals = y_server.perform_offline_precomputation_simplepir(None);

    // 3. Get expected row
    let target_row = 42;
    let expected: Vec<u64> = y_server
        .get_row(target_row)
        .iter()
        .map(|x| x.to_u64())
        .collect();

    // 4. Client operations via WASM API
    let params_json = params_to_json(&params);
    let mut client = ypir_wasm::ypir_client_init(&params_json);
    let query_bytes = ypir_wasm::ypir_generate_query(&mut client, target_row);

    // 5. Deserialize query and run server
    let (packed_query, pub_params) = ypir_wasm::deserialize_query(&params, &query_bytes);

    // Create AlignedMemory64 for the packed query
    let mut all_queries_packed = AlignedMemory64::new(params.db_rows_padded());
    (&mut all_queries_packed.as_mut_slice()[..packed_query.len()])
        .copy_from_slice(&packed_query);

    let pub_params_refs: Vec<&[PolyMatrixNTT]> =
        vec![pub_params.as_slice()];

    let mut measurement = Measurement::default();
    let response = y_server.perform_online_computation_simplepir(
        all_queries_packed.as_slice(),
        &offline_vals,
        &pub_params_refs,
        Some(&mut measurement),
    );

    // 6. Serialize response and decode via WASM API
    let response_bytes = ypir_wasm::serialize_response(&response);
    let decoded_bytes = ypir_wasm::ypir_decode_response(&client, &response_bytes);

    // 7. Parse decoded bytes as u64s and compare
    let num_rlwe_outputs = db_cols / params.poly_len;
    let decoded_u64s: Vec<u64> = decoded_bytes
        .chunks_exact(8)
        .map(|chunk| u64::from_le_bytes(chunk.try_into().unwrap()))
        .take(num_rlwe_outputs * params.poly_len)
        .collect();

    assert_eq!(decoded_u64s.len(), expected.len());
    assert_eq!(decoded_u64s, expected);
}

#[test]
fn bench_roundtrip_small() {
    let params = params_for_scenario_simplepir(2048, 2048 * 14);
    let db_rows = 1 << (params.db_dim_1 + params.poly_len_log2);
    let db_cols = params.instances * params.poly_len;

    let data: Vec<u16> = (0..(db_rows * db_cols) as u64)
        .map(|i| (i % params.pt_modulus) as u16)
        .collect();

    let t = Instant::now();
    let y_server = YServer::<u16>::new(&params, data.iter().copied(), true, false, true);
    eprintln!("  server setup:           {:>8.2?}", t.elapsed());

    let t = Instant::now();
    let offline_vals = y_server.perform_offline_precomputation_simplepir(None);
    eprintln!("  server offline precomp: {:>8.2?}", t.elapsed());

    let target_row = 42;
    let params_json = params_to_json(&params);

    let t = Instant::now();
    let mut client = ypir_wasm::ypir_client_init(&params_json);
    eprintln!("  client init:            {:>8.2?}", t.elapsed());

    let t = Instant::now();
    let query_bytes = ypir_wasm::ypir_generate_query(&mut client, target_row);
    eprintln!("  client query gen:       {:>8.2?}", t.elapsed());
    eprintln!("  query size:             {:>8} bytes", query_bytes.len());

    let t = Instant::now();
    let (packed_query, pub_params) = ypir_wasm::deserialize_query(&params, &query_bytes);
    eprintln!("  query deserialize:      {:>8.2?}", t.elapsed());

    let mut all_queries_packed = AlignedMemory64::new(params.db_rows_padded());
    (&mut all_queries_packed.as_mut_slice()[..packed_query.len()])
        .copy_from_slice(&packed_query);
    let pub_params_refs: Vec<&[PolyMatrixNTT]> = vec![pub_params.as_slice()];

    let t = Instant::now();
    let response = y_server.perform_online_computation_simplepir(
        all_queries_packed.as_slice(),
        &offline_vals,
        &pub_params_refs,
        None,
    );
    eprintln!("  server online compute:  {:>8.2?}", t.elapsed());

    let response_bytes = ypir_wasm::serialize_response(&response);
    eprintln!("  response size:          {:>8} bytes", response_bytes.len());

    let t = Instant::now();
    let decoded_bytes = ypir_wasm::ypir_decode_response(&client, &response_bytes);
    eprintln!("  client decode response: {:>8.2?}", t.elapsed());
    eprintln!("  decoded size:           {:>8} bytes", decoded_bytes.len());
}

#[test]
#[ignore] // Too large for CI / low-memory machines
fn test_roundtrip_cuckoo_shape() {
    // 65536 rows, 11 instances (cuckoo-index-like)
    let params = params_for_scenario_simplepir(65536, 65536 * 11 * 14);

    let db_rows = 1 << (params.db_dim_1 + params.poly_len_log2);
    let db_cols = params.instances * params.poly_len;

    let data: Vec<u16> = (0..(db_rows * db_cols) as u64)
        .map(|i| (i % params.pt_modulus) as u16)
        .collect();
    let y_server = YServer::<u16>::new(&params, data.iter().copied(), true, false, true);
    let offline_vals = y_server.perform_offline_precomputation_simplepir(None);

    let target_row = 100;
    let expected: Vec<u64> = y_server
        .get_row(target_row)
        .iter()
        .map(|x| x.to_u64())
        .collect();

    let params_json = params_to_json(&params);
    let mut client = ypir_wasm::ypir_client_init(&params_json);
    let query_bytes = ypir_wasm::ypir_generate_query(&mut client, target_row);
    let (packed_query, pub_params) = ypir_wasm::deserialize_query(&params, &query_bytes);

    let mut all_queries_packed = AlignedMemory64::new(params.db_rows_padded());
    (&mut all_queries_packed.as_mut_slice()[..packed_query.len()])
        .copy_from_slice(&packed_query);

    let pub_params_refs: Vec<&[PolyMatrixNTT]> = vec![pub_params.as_slice()];

    let response = y_server.perform_online_computation_simplepir(
        all_queries_packed.as_slice(),
        &offline_vals,
        &pub_params_refs,
        None,
    );

    let response_bytes = ypir_wasm::serialize_response(&response);
    let decoded_bytes = ypir_wasm::ypir_decode_response(&client, &response_bytes);

    let num_rlwe_outputs = db_cols / params.poly_len;
    let decoded_u64s: Vec<u64> = decoded_bytes
        .chunks_exact(8)
        .map(|chunk| u64::from_le_bytes(chunk.try_into().unwrap()))
        .take(num_rlwe_outputs * params.poly_len)
        .collect();

    assert_eq!(decoded_u64s, expected);
}
