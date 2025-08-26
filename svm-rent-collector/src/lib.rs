//! Solana SVM Rent Collector.
//!
//! Rent management for SVM.

pub mod rent_state;
pub mod svm_rent_collector;

pub use svm_rent_collector::rent_collector::{
    CollectedInfo, RentCollector, RENT_EXEMPT_RENT_EPOCH,
};
