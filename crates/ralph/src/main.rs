use clap::Parser;

/// Ralph - CLI tool for managing development workflows
#[derive(Parser, Debug)]
#[command(version, about, long_about = None)]
struct Args {}

fn main() {
    let _args = Args::parse();
    println!("Hello, Ralph!");
}
