#![feature(rustc_private)]
#![warn(unused_extern_crates)]
#![feature(box_patterns)]

extern crate rustc_ast;
extern crate rustc_data_structures;
extern crate rustc_hir;
extern crate rustc_lint;
extern crate rustc_middle;
extern crate rustc_span;

pub mod cpi_types;
pub mod diag_items;
pub mod mir_analyzer;
pub mod models;
pub mod utils;
