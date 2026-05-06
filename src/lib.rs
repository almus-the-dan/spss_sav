#![warn(missing_docs)]

//! A pure Rust reader and writer for SPSS data formats.
//!
//! Currently in scope: the **SAV/ZSAV** binary format ([`spss::sav`]).
//! Format-agnostic SPSS-domain types — values, missing-value
//! representations, encodings, and the temporal helpers — live at
//! [`spss`] and are shared across formats (a future POR
//! implementation would slot in alongside [`spss::sav`]).

/// SPSS file format types and utilities.
pub mod spss;
