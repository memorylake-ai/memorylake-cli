//! Application configuration placeholders.

/// Runtime configuration for MemoryLake clients and CLI commands.
///
/// Concrete sources (env, files, flags) will be wired in later.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Config {}

impl Config {
    /// Create an empty default configuration.
    pub fn new() -> Self {
        Self::default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_config_is_empty() {
        assert_eq!(Config::new(), Config::default());
    }
}
