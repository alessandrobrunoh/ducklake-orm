pub mod hello;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Command {
    Hello,
    Help,
}

pub fn parse_command<I>(args: I) -> Result<Command, String>
where
    I: IntoIterator<Item = String>,
{
    let mut args = args.into_iter();
    let _bin = args.next();

    match args.next().as_deref() {
        Some("hello") => Ok(Command::Hello),
        Some("help") | Some("--help") | Some("-h") | None => Ok(Command::Help),
        Some(other) => Err(format!(
            "Unknown command: {other}\nRun `ducklake-cli help` to see available commands."
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::{Command, parse_command};

    #[test]
    fn parses_hello_command() {
        let args = vec!["ducklake-cli".to_string(), "hello".to_string()];
        let command = parse_command(args).unwrap();

        assert_eq!(command, Command::Hello);
    }

    #[test]
    fn defaults_to_help_without_args() {
        let args = vec!["ducklake-cli".to_string()];
        let command = parse_command(args).unwrap();

        assert_eq!(command, Command::Help);
    }
}
