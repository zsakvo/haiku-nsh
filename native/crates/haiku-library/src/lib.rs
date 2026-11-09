//! Haiku library domain boundary.
pub mod local_library;
pub mod local_scan;
pub mod tags;

#[cfg(any(test, feature = "test-support"))]
pub mod test_support;
