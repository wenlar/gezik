use std::fmt;

/// A problem found in a settings or theme file. Never fatal: the caller falls back to a
/// sensible value and shows the warning to the user.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Warning {
    /// The file the problem is in, e.g. `nord.toml`.
    pub file: String,
    /// 1-based line, when known.
    pub line: Option<usize>,
    pub message: String,
}

impl Warning {
    pub fn new(file: impl Into<String>, message: impl Into<String>) -> Self {
        Self { file: file.into(), line: None, message: message.into() }
    }

    /// A warning for a TOML syntax error, with the error's byte offset turned into a line.
    pub fn from_toml_error(file: impl Into<String>, text: &str, err: &toml::de::Error) -> Self {
        Self {
            file: file.into(),
            line: err.span().map(|span| line_of(text, span.start)),
            message: err.message().to_string(),
        }
    }
}

fn line_of(text: &str, offset: usize) -> usize {
    text.as_bytes()[..offset.min(text.len())].iter().filter(|&&b| b == b'\n').count() + 1
}

impl fmt::Display for Warning {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.line {
            Some(line) => write!(f, "{} line {}: {}", self.file, line, self.message),
            None => write!(f, "{}: {}", self.file, self.message),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::Warning;

    #[test]
    fn displays_with_and_without_line() {
        let mut warning = Warning::new("nord.toml", "colors.accent: bad value");
        assert_eq!(warning.to_string(), "nord.toml: colors.accent: bad value");
        warning.line = Some(12);
        assert_eq!(warning.to_string(), "nord.toml line 12: colors.accent: bad value");
    }

    #[test]
    fn syntax_error_reports_line_and_message() {
        let text = "a = 1\nb = [\n";
        let err = text.parse::<toml::Table>().unwrap_err();
        let warning = Warning::from_toml_error("x.toml", text, &err);
        assert_eq!(warning.file, "x.toml");
        assert_eq!(warning.line, Some(2));
        assert!(warning.message.contains("unclosed array"), "{}", warning.message);
    }

    #[test]
    fn crlf_files_count_lines_the_same() {
        let text = "a = 1\r\nb = [\r\n";
        let err = text.parse::<toml::Table>().unwrap_err();
        assert_eq!(Warning::from_toml_error("x.toml", text, &err).line, Some(2));
    }
}
