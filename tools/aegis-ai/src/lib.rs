//! `aegis-ai`: Dataset compiler, DSL parser, and `aegis-model` verifier for
//! `AegisTARA`'s in-house compact TARA & attack-tree AI models.

#![forbid(unsafe_code)]
#![warn(missing_docs)]
#![warn(clippy::pedantic)]

pub mod corpus;
pub mod dataset;
pub mod dpo;
pub mod dsl;
pub mod predictor;
pub mod verifier;
