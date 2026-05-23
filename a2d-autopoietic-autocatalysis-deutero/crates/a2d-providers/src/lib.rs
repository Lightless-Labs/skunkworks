//! Concrete LLM provider implementations for A²D.
//!
//! Each provider wraps a model API and implements the `Provider` trait
//! from a2d-core. Providers are stateless — no conversation history.

pub mod claude;
pub mod cli;
