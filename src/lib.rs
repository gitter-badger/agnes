/*!
Data management library for Rust. Provides data structs and utilities for data aggregation,
manipulation, and viewing.
*/

#![warn(missing_docs)]
#![deny(bare_trait_objects, unconditional_recursion)]

extern crate bit_vec;
extern crate csv;
extern crate encoding;
extern crate futures;
extern crate hyper;
extern crate hyper_tls;
extern crate indexmap;
extern crate native_tls;
extern crate num_traits;
extern crate serde;
extern crate tokio_core;
extern crate tokio_io;
#[macro_use]
extern crate prettytable;
extern crate csv_sniffer;
extern crate tempfile;
extern crate typenum;

#[cfg(test)]
extern crate rand;
#[cfg(test)]
extern crate serde_json;

#[macro_use]
pub mod ops;
#[macro_use]
pub mod cons;
#[macro_use]
pub mod partial;
#[macro_use]
pub mod label;
#[macro_use]
pub mod fieldlist;
#[macro_use]
pub mod store;
#[macro_use]
pub mod field;

#[cfg(feature = "test-utils")]
#[macro_use]
pub mod test_utils;

pub mod access;
pub mod error;
pub mod frame;
pub mod join;
pub mod select;
pub mod source;
pub mod stats;
pub mod view;
pub mod view_stats;
// pub mod reshape;

#[cfg(feature = "experimental")]
pub mod experimental;

#[cfg(test)]
pub mod test_gen_data;
