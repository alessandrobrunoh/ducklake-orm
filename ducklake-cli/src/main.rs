mod app;
mod commands;

fn main() {
    if let Err(error) = app::run(std::env::args()) {
        eprintln!("{error}");
        std::process::exit(1);
    }
}
