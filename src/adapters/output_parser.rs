#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum LogType {
    Warning,
    Error,
}

pub fn parse_log_type(line: &str, prefixes: &[(LogType, &[&str])]) -> Option<(LogType, String)> {
    let line_trim = line.trim();
    for (log_type, variants) in prefixes {
        for prefix in *variants {
            if line_trim.starts_with(prefix) {
                let content = line_trim.strip_prefix(prefix).unwrap().trim().to_string();
                return Some((*log_type, content));
            }
        }
    }
    None
}

pub fn strip_repeated_prefix<'a>(mut line: &'a str, prefix: &str) -> &'a str {
    while line.starts_with(prefix) {
        line = line.strip_prefix(prefix).unwrap().trim_start();
    }
    line
}

#[cfg(test)]
mod tests {
    use super::*;

    const PREFIXES: &[(LogType, &[&str])] = &[
        (LogType::Warning, &["WARN:", "warning:"]),
        (LogType::Error, &["ERROR:", "error:"]),
    ];

    #[test]
    fn parses_warning_prefixes() {
        let parsed = parse_log_type("WARN: something happened", PREFIXES);
        assert_eq!(
            parsed,
            Some((LogType::Warning, "something happened".to_string()))
        );
    }

    #[test]
    fn parses_error_prefixes() {
        let parsed = parse_log_type("error: build failed", PREFIXES);
        assert_eq!(parsed, Some((LogType::Error, "build failed".to_string())));
    }

    #[test]
    fn strips_repeated_tool_prefix() {
        let stripped = strip_repeated_prefix("ohpm ohpm WARN: duplicate", "ohpm ");
        assert_eq!(stripped, "WARN: duplicate");
    }
}
