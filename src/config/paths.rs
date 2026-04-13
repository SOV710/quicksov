// SPDX-FileCopyrightText: 2026 SOV710
//
// SPDX-License-Identifier: GPL-3.0-or-later

use thiserror::Error;

/// Errors encountered while expanding environment variables in path strings.
#[derive(Debug, Error)]
pub enum PathsError {
    #[error("environment variable ${0} is not set (required for path expansion)")]
    EnvVarNotSet(String),
}

/// Expand `$VAR` references in `s` using the process environment.
///
/// Scans for `$` followed by alphanumeric-and-underscore characters and replaces
/// each occurrence with `std::env::var`. Returns an error if any referenced
/// variable is not set in the environment.
pub fn expand_env_vars(s: &str) -> Result<String, PathsError> {
    let mut result = String::with_capacity(s.len());
    let bytes = s.as_bytes();
    let mut i = 0;

    while i < bytes.len() {
        if bytes[i] == b'$' {
            i += 1;
            let start = i;
            while i < bytes.len() && (bytes[i].is_ascii_alphanumeric() || bytes[i] == b'_') {
                i += 1;
            }
            let var_name = &s[start..i];
            if var_name.is_empty() {
                result.push('$');
            } else {
                let val = std::env::var(var_name)
                    .map_err(|_| PathsError::EnvVarNotSet(var_name.to_string()))?;
                result.push_str(&val);
            }
        } else {
            result.push(bytes[i] as char);
            i += 1;
        }
    }

    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn expand_plain_string_unchanged() {
        assert_eq!(
            expand_env_vars("/run/user/1000/foo.sock").unwrap(),
            "/run/user/1000/foo.sock"
        );
    }

    #[test]
    fn expand_missing_var_errors() {
        std::env::remove_var("__QUICKSOV_NONEXISTENT_VAR__");
        let err = expand_env_vars("$__QUICKSOV_NONEXISTENT_VAR__/foo");
        assert!(err.is_err());
    }
}
