use wasm_bindgen::prelude::*;

use rand::SeedableRng;
use rand_chacha::ChaCha20Rng;

use spiral_rs::client::Client;
use spiral_rs::params::Params;
use spiral_rs::poly::{PolyMatrix, PolyMatrixRaw};

use ypir::client::{decrypt_ct_reg_measured, pack_query, raw_generate_expansion_params, YClient};
use ypir::modulus_switch::ModulusSwitch;
use ypir::packing::condense_matrix;
use ypir::params::{ext_params_from_json, GetQPrime};
use ypir::scheme::{SEED_0, STATIC_SEED_2};

/// Opaque handle holding the YPIR client state.
///
/// Params are Box::leaked to get 'static lifetime, which is needed because
/// Client<'a> borrows &'a Params. This is acceptable in WASM where page
/// refresh cleans up all memory.
#[wasm_bindgen]
pub struct YpirWasmClient {
    params: &'static Params,
    client: Client<'static>,
}

/// Initialize a YPIR client from a JSON-serialized Params.
///
/// The JSON format matches ypir's ext_params_from_json (fields: n, nu_1, nu_2,
/// p, q2_bits, t_gsw, t_conv, t_exp_left, t_exp_right, instances, db_item_size,
/// version, poly_len, moduli, noise_width).
#[wasm_bindgen]
pub fn ypir_client_init(params_json: &str) -> YpirWasmClient {
    let params = ext_params_from_json(params_json);
    let params_static: &'static Params = Box::leak(Box::new(params));

    let mut client = Client::init(params_static);
    client.generate_secret_keys();

    YpirWasmClient {
        params: params_static,
        client,
    }
}

/// Generate a query for a given target row.
///
/// Returns a byte vector containing the packed query and condensed pub params
/// in a length-prefixed binary format:
///
/// ```text
/// [4B LE u32] packed_query.len() (number of u64 elements)
/// [N bytes]   packed_query (as raw u64 LE bytes)
/// [4B LE u32] num_pub_params
/// For each pub_param:
///   [4B LE u32] rows
///   [4B LE u32] cols
///   [4B LE u32] data.len() (number of u64 elements)
///   [M bytes]   polynomial matrix data (u64 LE bytes)
/// ```
#[wasm_bindgen]
pub fn ypir_generate_query(client: &mut YpirWasmClient, target_row: usize) -> Vec<u8> {
    let params = client.params;

    let sk_reg = client.client.get_sk_reg();
    let pack_pub_params = raw_generate_expansion_params(
        params,
        &sk_reg,
        params.poly_len_log2,
        params.t_exp_left,
        &mut ChaCha20Rng::from_entropy(),
        &mut ChaCha20Rng::from_seed(STATIC_SEED_2),
    );

    // Extract row 1 of each pub param matrix and condense
    let pack_pub_params_row_1s: Vec<_> = pack_pub_params
        .iter()
        .map(|pp| {
            let row1 = pp.submatrix(1, 0, 1, pp.cols);
            condense_matrix(params, &row1)
        })
        .collect();

    // Generate the RLWE query
    // We need to use a raw pointer dance here because YClient::new takes &'a mut Client<'a>
    // and Rust's borrow checker is strict about mutable reference invariance with 'static
    let client_ptr = &mut client.client as *mut Client<'static>;
    let y_client = YClient::new(unsafe { &mut *client_ptr }, params);
    let query_row = y_client.generate_query(SEED_0, params.db_dim_1, true, target_row);
    let packed_query = pack_query(params, &query_row);
    let packed_query_data = packed_query.as_slice();

    // Serialize to bytes
    let mut out = Vec::new();

    // Packed query
    let query_len = packed_query_data.len() as u32;
    out.extend_from_slice(&query_len.to_le_bytes());
    for &val in packed_query_data {
        out.extend_from_slice(&val.to_le_bytes());
    }

    // Pub params
    let num_pub_params = pack_pub_params_row_1s.len() as u32;
    out.extend_from_slice(&num_pub_params.to_le_bytes());
    for pp in &pack_pub_params_row_1s {
        let rows = pp.rows as u32;
        let cols = pp.cols as u32;
        let data = pp.as_slice();
        let data_len = data.len() as u32;
        out.extend_from_slice(&rows.to_le_bytes());
        out.extend_from_slice(&cols.to_le_bytes());
        out.extend_from_slice(&data_len.to_le_bytes());
        for &val in data {
            out.extend_from_slice(&val.to_le_bytes());
        }
    }

    out
}

/// Decode a server response back into plaintext bytes.
///
/// `response_bytes` contains the modulus-switched ciphertexts concatenated together.
/// Each ciphertext is `(ceil(q_prime_2_bits/8) + ceil(q_prime_1_bits/8)) * poly_len` bytes.
#[wasm_bindgen]
pub fn ypir_decode_response(client: &YpirWasmClient, response_bytes: &[u8]) -> Vec<u8> {
    let params = client.params;
    let rlwe_q_prime_1 = params.get_q_prime_1();
    let rlwe_q_prime_2 = params.get_q_prime_2();

    let q_prime_1_bits = (rlwe_q_prime_2 as f64).log2().ceil() as usize;
    let q_prime_2_bits = (rlwe_q_prime_1 as f64).log2().ceil() as usize;
    let ct_byte_size = ((q_prime_1_bits + q_prime_2_bits) * params.poly_len + 7) / 8;

    let mut result = Vec::new();
    for ct_bytes in response_bytes.chunks(ct_byte_size) {
        if ct_bytes.len() < ct_byte_size {
            break;
        }
        let ct = PolyMatrixRaw::recover(params, rlwe_q_prime_1, rlwe_q_prime_2, ct_bytes);
        let decrypted =
            decrypt_ct_reg_measured(&client.client, params, &ct.ntt(), params.poly_len);
        for &coeff in decrypted.data.as_slice() {
            result.extend_from_slice(&coeff.to_le_bytes());
        }
    }

    result
}

// Non-wasm helper functions for native tests

/// Deserialize query bytes (produced by ypir_generate_query) into packed query u64s
/// and pub param polynomial matrices.
///
/// Returns (packed_query, pub_params_condensed). The pub params are in condensed form,
/// which is how the server expects them.
#[cfg(not(target_arch = "wasm32"))]
pub fn deserialize_query<'a>(
    params: &'a Params,
    query_bytes: &[u8],
) -> (Vec<u64>, Vec<spiral_rs::poly::PolyMatrixNTT<'a>>) {
    let mut offset = 0;

    // Read packed query
    let query_len =
        u32::from_le_bytes(query_bytes[offset..offset + 4].try_into().unwrap()) as usize;
    offset += 4;
    let mut packed_query = Vec::with_capacity(query_len);
    for _ in 0..query_len {
        let val = u64::from_le_bytes(query_bytes[offset..offset + 8].try_into().unwrap());
        offset += 8;
        packed_query.push(val);
    }

    // Read pub params (kept in condensed form for the server)
    let num_pub_params =
        u32::from_le_bytes(query_bytes[offset..offset + 4].try_into().unwrap()) as usize;
    offset += 4;
    let mut pub_params = Vec::with_capacity(num_pub_params);
    for _ in 0..num_pub_params {
        let rows =
            u32::from_le_bytes(query_bytes[offset..offset + 4].try_into().unwrap()) as usize;
        offset += 4;
        let cols =
            u32::from_le_bytes(query_bytes[offset..offset + 4].try_into().unwrap()) as usize;
        offset += 4;
        let data_len =
            u32::from_le_bytes(query_bytes[offset..offset + 4].try_into().unwrap()) as usize;
        offset += 4;
        let mut pm = spiral_rs::poly::PolyMatrixNTT::zero(params, rows, cols);
        assert_eq!(pm.as_slice().len(), data_len);
        for i in 0..data_len {
            let val = u64::from_le_bytes(query_bytes[offset..offset + 8].try_into().unwrap());
            offset += 8;
            pm.as_mut_slice()[i] = val;
        }
        pub_params.push(pm);
    }

    (packed_query, pub_params)
}

/// Serialize a server response (Vec<Vec<u8>>) into a flat byte vector.
#[cfg(not(target_arch = "wasm32"))]
pub fn serialize_response(response: &[Vec<u8>]) -> Vec<u8> {
    let mut out = Vec::new();
    for ct_bytes in response {
        out.extend_from_slice(ct_bytes);
    }
    out
}

/// Serialize a `Params` struct to the JSON format expected by `ypir_client_init`.
///
/// This is useful on the server side to generate the params JSON that gets sent
/// to the WASM client.
#[cfg(not(target_arch = "wasm32"))]
pub fn params_to_json(params: &Params) -> String {
    serde_json::json!({
        "n": params.n,
        "nu_1": params.db_dim_1,
        "nu_2": params.db_dim_2,
        "p": params.pt_modulus,
        "q2_bits": params.q2_bits,
        "t_gsw": params.t_gsw,
        "t_conv": params.t_conv,
        "t_exp_left": params.t_exp_left,
        "t_exp_right": params.t_exp_right,
        "instances": params.instances,
        "db_item_size": params.db_item_size,
        "version": params.version,
        "poly_len": params.poly_len,
        "moduli": [params.moduli[0].to_string(), params.moduli[1].to_string()],
        "noise_width": params.noise_width
    })
    .to_string()
}
