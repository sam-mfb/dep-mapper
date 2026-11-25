//! Turns a `BuildSelection` into the Cargo flags that select the same build.

use project_graph::{BuildSelection, GraphingError};

/// The package the selection names.
pub fn package(selection: &BuildSelection) -> &str {
    selection
        .target
        .split_once('/')
        .map_or(selection.target.as_str(), |(package, _)| package)
}

pub fn cargo_args(selection: &BuildSelection) -> Result<Vec<String>, GraphingError> {
    let mut args = vec!["-p".to_string(), package(selection).to_string()];
    if let Some((_, target)) = selection.target.split_once('/') {
        args.extend(target_args(target)?);
    }
    args.extend(configuration_args(&selection.configuration)?);
    Ok(args)
}

fn target_args(target: &str) -> Result<Vec<String>, GraphingError> {
    let invalid = || GraphingError {
        message: format!(
            "target `{target}` is not `kind:name` with kind lib, bin, test, example or bench"
        ),
    };
    let (kind, name) = target.split_once(':').ok_or_else(invalid)?;
    Ok(match kind {
        "lib" => vec!["--lib".to_string()],
        "bin" | "test" | "example" | "bench" if !name.is_empty() => {
            vec![format!("--{kind}"), name.to_string()]
        }
        _ => return Err(invalid()),
    })
}

fn configuration_args(configuration: &str) -> Result<Vec<String>, GraphingError> {
    if configuration.trim() == "default features" {
        return Ok(Vec::new());
    }
    let mut words = configuration.split_whitespace();
    let mut args = Vec::new();
    while let Some(word) = words.next() {
        match word {
            "--no-default-features" | "--all-features" => args.push(word.to_string()),
            "--features" | "--target" => {
                let value = words.next().ok_or_else(|| GraphingError {
                    message: format!("`{word}` needs a value"),
                })?;
                args.extend([word.to_string(), value.to_string()]);
            }
            _ => {
                return Err(GraphingError {
                    message: format!(
                        "configuration `{configuration}`: `{word}` is not `default features` \
                         or one of --features, --no-default-features, --all-features, --target"
                    ),
                });
            }
        }
    }
    Ok(args)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn selection(target: &str, configuration: &str) -> BuildSelection {
        BuildSelection {
            root: PathBuf::from("/project"),
            target: target.into(),
            configuration: configuration.into(),
        }
    }

    #[test]
    fn a_package_with_default_features_selects_only_the_package() {
        assert_eq!(
            cargo_args(&selection("app", "default features")).unwrap(),
            ["-p", "app"]
        );
    }

    #[test]
    fn a_target_and_flags_become_cargo_flags() {
        let args = cargo_args(&selection(
            "app/bin:serve",
            "--no-default-features --features tls,metrics --target x86_64-unknown-linux-gnu",
        ))
        .unwrap();
        assert_eq!(
            args,
            [
                "-p",
                "app",
                "--bin",
                "serve",
                "--no-default-features",
                "--features",
                "tls,metrics",
                "--target",
                "x86_64-unknown-linux-gnu"
            ]
        );
        assert_eq!(
            cargo_args(&selection("app/lib:app", "default features")).unwrap()[2],
            "--lib"
        );
    }

    #[test]
    fn unknown_flags_and_malformed_targets_are_refused() {
        assert!(cargo_args(&selection("app", "--release")).is_err());
        assert!(cargo_args(&selection("app", "--features")).is_err());
        assert!(cargo_args(&selection("app/binary:x", "default features")).is_err());
        assert!(cargo_args(&selection("app/bin:", "default features")).is_err());
    }
}
