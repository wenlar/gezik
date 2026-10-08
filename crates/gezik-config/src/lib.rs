//! Settings and themes for Gezik. Knows nothing about the UI: inputs are files and
//! text, outputs are validated values plus warnings to show the user.

pub mod batch_toml;
pub mod paths;
pub mod pins;
pub mod settings;
pub mod settings_edit;
pub mod settings_writer;
pub mod shortcuts;
mod state_store;
pub mod store;
pub mod theme;
pub mod theme_rules;
pub mod views_file;
mod views_writer;

mod color;
mod warning;

pub use color::Color;
pub use warning::Warning;

/// Locks `mutex`, going on with its data even if a thread panicked while holding it.
pub fn lock<T>(mutex: &std::sync::Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(std::sync::PoisonError::into_inner)
}

/// A fresh, empty folder for a test.
#[cfg(test)]
pub(crate) fn test_dir(name: &str) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!("gezik-config-test-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}
