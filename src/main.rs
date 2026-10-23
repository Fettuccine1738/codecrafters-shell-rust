#[allow(unused_imports)]
use std::io::{self, Write};

static EXIT_SHELL_KEYWORD: &'static str = "exit";

fn repl() -> Result<(), io::Error> {
    loop {
        print!("$ ");
        io::stdout().flush().unwrap();
        let mut buf = String::new();
        io::stdin().read_line(&mut buf)?;
        let buf = buf.trim();

        if buf == EXIT_SHELL_KEYWORD {
            std::process::exit(0);
        }

        println!("{}: command not found", buf);
    }
}

fn main() {
    match repl() {
        Ok(()) => (),
        Err(_err) => todo!(),
    }
}
