//! Reads a command's arguments: positional values and `--name value` options.

use super::CommandError;

pub(super) struct Args {
    positional: std::collections::VecDeque<String>,
    options: Vec<(String, String)>,
    error: Option<String>,
}

impl Args {
    pub(super) fn new(arguments: &[String]) -> Args {
        let mut positional = std::collections::VecDeque::new();
        let mut options = Vec::new();
        let mut error = None;
        let mut iter = arguments.iter();
        while let Some(argument) = iter.next() {
            if argument.starts_with("--") {
                match iter.next() {
                    Some(value) => options.push((argument.clone(), value.clone())),
                    None => error = Some(format!("{argument} needs a value")),
                }
            } else {
                positional.push_back(argument.clone());
            }
        }
        Args {
            positional,
            options,
            error,
        }
    }

    pub(super) fn next_positional(&mut self) -> Option<String> {
        self.positional.pop_front()
    }

    pub(super) fn required_positional(&mut self, what: &str) -> Result<String, CommandError> {
        self.next_positional()
            .ok_or_else(|| CommandError::Usage(format!("give {what}")))
    }

    pub(super) fn rest_positional(&mut self) -> Vec<String> {
        self.positional.drain(..).collect()
    }

    /// An option given at most once.
    pub(super) fn optional(&mut self, name: &str) -> Result<Option<String>, CommandError> {
        let mut values = self.all(name);
        match values.len() {
            0 => Ok(None),
            1 => Ok(values.pop()),
            _ => Err(CommandError::Usage(format!("give {name} once"))),
        }
    }

    pub(super) fn required(&mut self, name: &str) -> Result<String, CommandError> {
        self.optional(name)?
            .ok_or_else(|| CommandError::Usage(format!("give {name}")))
    }

    /// Every value of an option that may repeat, in order.
    pub(super) fn all(&mut self, name: &str) -> Vec<String> {
        let (taken, kept) = std::mem::take(&mut self.options)
            .into_iter()
            .partition(|(option, _)| option == name);
        self.options = kept;
        taken.into_iter().map(|(_, value)| value).collect()
    }

    /// Fails if anything was malformed or left unused.
    pub(super) fn finish(&mut self) -> Result<(), CommandError> {
        if let Some(error) = self.error.take() {
            return Err(CommandError::Usage(error));
        }
        if let Some((option, _)) = self.options.first() {
            return Err(CommandError::Usage(format!("unknown option {option}")));
        }
        if let Some(extra) = self.positional.front() {
            return Err(CommandError::Usage(format!(
                "unexpected argument {extra:?}"
            )));
        }
        Ok(())
    }
}
