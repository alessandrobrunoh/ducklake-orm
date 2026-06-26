use crate::commands::{Command, parse_command};

pub fn run<I>(args: I) -> Result<(), String>
where
    I: IntoIterator<Item = String>,
{
    match parse_command(args)? {
        Command::Hello => {
            crate::commands::hello::execute();
            Ok(())
        }
        Command::Help => {
            print_help();
            Ok(())
        }
    }
}

fn print_help() {
    println!("ducklake-cli");
    println!();
    println!("USAGE:");
    println!("  ducklake-cli <COMMAND>");
    println!();
    println!("COMMANDS:");
    println!("  hello    Print \"Hello World\"");
    println!("  help     Show this help message");
}
