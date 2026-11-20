//! Diagnostics reported by manifest validation and shader compilation (spec §5.4, §16).

use std::fmt;

/// How bad a diagnostic is.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Severity {
    /// The plugin (or the contribution) still loads.
    Warning,
    /// The plugin (or the contribution) does not load.
    Error,
}

/// One problem found in a plugin package.
///
/// `file` is package-relative (`bae-plugin.toml`, `shaders/vignette.wgsl`);
/// `line`/`column` are 1-based when known. `context` names the manifest entry
/// (`effect "vignette", param "amount"`) for manifest-level problems.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Diagnostic {
    /// Severity.
    pub severity: Severity,
    /// Package-relative file.
    pub file: String,
    /// 1-based line, if known.
    pub line: Option<u32>,
    /// 1-based column (bytes), if known.
    pub column: Option<u32>,
    /// Manifest entry the problem belongs to; empty for file-level problems.
    pub context: String,
    /// What is wrong, in one sentence.
    pub message: String,
}

impl Diagnostic {
    /// An error in `file`.
    pub fn error(file: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            severity: Severity::Error,
            file: file.into(),
            line: None,
            column: None,
            context: String::new(),
            message: message.into(),
        }
    }

    /// A warning in `file`.
    pub fn warning(file: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            severity: Severity::Warning,
            ..Self::error(file, message)
        }
    }

    /// Sets the manifest context.
    pub fn in_context(mut self, context: impl Into<String>) -> Self {
        self.context = context.into();
        self
    }

    /// Sets the location.
    pub fn at(mut self, line: u32, column: u32) -> Self {
        self.line = Some(line);
        self.column = Some(column);
        self
    }

    /// `true` for errors.
    pub fn is_error(&self) -> bool {
        self.severity == Severity::Error
    }
}

impl fmt::Display for Diagnostic {
    /// `file:line:column: error: context: message` (compiler style, so editors
    /// and CI logs link to the location).
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.file)?;
        if let Some(line) = self.line {
            write!(f, ":{line}")?;
            if let Some(column) = self.column {
                write!(f, ":{column}")?;
            }
        }
        let severity = match self.severity {
            Severity::Warning => "warning",
            Severity::Error => "error",
        };
        write!(f, ": {severity}: ")?;
        if !self.context.is_empty() {
            write!(f, "{}: ", self.context)?;
        }
        f.write_str(&self.message)
    }
}

/// `true` if any diagnostic is an error.
pub fn has_errors(diagnostics: &[Diagnostic]) -> bool {
    diagnostics.iter().any(Diagnostic::is_error)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn display_is_compiler_style() {
        let d = Diagnostic::error("shaders/a.wgsl", "unknown identifier `x`").at(3, 7);
        assert_eq!(
            d.to_string(),
            "shaders/a.wgsl:3:7: error: unknown identifier `x`"
        );
        let d =
            Diagnostic::warning("bae-plugin.toml", "no effects").in_context("effect \"vignette\"");
        assert_eq!(
            d.to_string(),
            "bae-plugin.toml: warning: effect \"vignette\": no effects"
        );
        assert!(has_errors(&[d.clone(), Diagnostic::error("x", "y")]));
        assert!(!has_errors(&[d]));
    }
}
