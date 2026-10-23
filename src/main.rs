#[allow(unused_imports)]
use std::io::{self, Write};

static KEYWORD_EXIT_SHELL: &'static str = "exit";
static KEYWORD_ECHO: &'static str = "echo";
static KEYWORD_TYPE: &'static str = "type";
static KEYWORD_BUILTIN: &'static str = "builtin";
static LIST_VALID_CMDS: [&'static str; 4] = [
    KEYWORD_BUILTIN,
    KEYWORD_ECHO,
    KEYWORD_EXIT_SHELL,
    KEYWORD_TYPE,
];

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
    } else if keyword == KEYWORD_TYPE {
        let cmd = rest;
        let mut valid = false;

        for s in LIST_VALID_CMDS {
            if s == cmd {
                valid = true;
                break;
            }
        }

        if valid {
            println!("{cmd} is a shell builtin");
        } else {
            println!("{cmd}: not found");
        }
        Ok(())
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
