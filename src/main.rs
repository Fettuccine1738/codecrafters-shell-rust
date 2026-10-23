use codecrafters_shell::repl;
use std::env;

fn main() {
    let _args: Vec<String> = env::args().skip(1).collect();
    match repl() {
        Ok(()) => (),
        Err(_err) => todo!(),
    }
}
