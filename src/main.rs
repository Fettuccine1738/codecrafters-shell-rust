#[allow(unused_imports)]
use std::io::{self, Write};

static KEYWORD_EXIT_SHELL: &'static str = "exit";
static KEYWORD_ECHO: &'static str = "echo";

fn repl() -> Result<(), io::Error> {
    loop {
        print!("$ ");
        io::stdout().flush().unwrap();
        let mut buf = String::new();
        io::stdin().read_line(&mut buf)?;
        let buf = buf.trim();

        let _ = eval(buf);
    }
}

pub struct EvalError;

fn eval(input: &str) -> Result<(), EvalError> {
    let input = input.trim();
    if input.is_empty() {
        return Ok(());
    }

    let (keyword, rest) = match input.find(|ch: char| ch.is_whitespace()) {
        Some(index) => {
            let (k, r) = input.split_at(index);
            (k, r.trim())
        }
        None => (input, ""),
    };

    if keyword == KEYWORD_EXIT_SHELL {
        std::process::exit(0);
    } else if keyword == KEYWORD_ECHO {
        println!("{rest}");
        return Ok(());
    } else {
        println!("{}: command not found", keyword);
        return Ok(());
    }
}

fn main() {
    match repl() {
        Ok(()) => (),
        Err(_err) => todo!(),
    }
}
