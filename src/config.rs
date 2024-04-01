use std::{env::Args, process::exit};

use self::help::print_help;

mod help;

pub struct Config {
    pub query: Option<String>,
}

impl Config {
    pub fn new(args: Args) -> Self {
        let mut query: Option<String> = None;

        for arg in args.skip(1) {
            match arg.as_str() {
                "-h" | "--help" => print_help(),
                #[allow(clippy::option_if_let_else)]
                _ => {
                    if let Some(q) = query {
                        query = Some(q + " " + &arg);
                    } else {
                        query = Some(arg.to_owned());
                    }
                }
            }
        }

        if let Err(err) = std::process::Command::new("brew").arg("--help").output() {
            match err.kind() {
                std::io::ErrorKind::NotFound => {
                    eprintln!("breweri: brew not found");
                }
                _ => {
                    eprintln!("breweri: {err}");
                }
            }
            exit(1);
        }

        Self { query }
    }
}
