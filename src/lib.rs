#![cfg_attr(target_arch = "x86_64", feature(stdarch_x86_avx512))]

pub mod bits;
pub mod client;
pub mod convolution;
#[cfg(feature = "server")]
pub mod kernel;
pub mod lwe;
#[cfg(feature = "server")]
pub mod matmul;
pub mod measurement;
pub mod modulus_switch;
pub mod noise_analysis;
pub mod packing;
pub mod params;
pub mod scheme;
#[cfg(feature = "server")]
pub mod server;
#[cfg(feature = "server")]
pub mod transpose;
pub mod util;
