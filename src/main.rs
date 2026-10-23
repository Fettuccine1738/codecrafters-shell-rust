#[allow(unused_imports)]
use std::io::{self, Write};
use std::{
    env::{self},
    fs::{self},
    path::{Path, PathBuf},
};

use is_executable::is_executable;

static KEYWORD_EXIT_SHELL: &str = "exit";
static KEYWORD_ECHO: &str = "echo";
static KEYWORD_TYPE: &str = "type";
static KEYWORD_BUILTIN: &str = "builtin";
static LIST_VALID_CMDS: [&str; 4] = [
    KEYWORD_BUILTIN,
    KEYWORD_ECHO,
    KEYWORD_EXIT_SHELL,
    KEYWORD_TYPE,
];

fn repl() -> Result<(), io::Error> {
    let paths = load_paths();
    // dbg!(&paths);
    loop {
        print!("$ ");
        io::stdout().flush().unwrap();
        let mut buf = String::new();
        io::stdin().read_line(&mut buf)?;
        let buf = buf.trim();

        let _ = eval(buf, &paths);
    }
}

pub struct EvalError;

fn eval(input: &str, paths: &[PathBuf]) -> Result<(), EvalError> {
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
        Ok(())
    } else if keyword == KEYWORD_TYPE {
        let cmd = rest;
        if find_in_cmds(cmd) {
            println!("{cmd} is a shell builtin");
            return Ok(());
        }

        match find_exec_file(cmd, paths) {
            Some(pathbuf) => {
                println!("{cmd} is {}", pathbuf.to_str().unwrap());
            }
            None => {
                println!("{cmd}: not found");
            }
        }
        Ok(())
    } else {
        println!("{}: command not found", keyword);
        Ok(())
    }
}

fn find_in_cmds(to_find: &str) -> bool {
    for s in LIST_VALID_CMDS {
        if s == to_find {
            return true;
        }
    }
    false
}

fn find_exec_file(to_find: &str, paths: &[PathBuf]) -> Option<PathBuf> {
    for path in paths {
        let dir = fs::read_dir(path).unwrap();
        for entry in dir {
            // dbg!(&entry);
            match entry {
                Ok(dir_entry) => {
                    let tk = dir_entry.file_name().into_string().unwrap();
                    if is_executable(dir_entry.path()) {
                        let tokens: Vec<&str> = Path::new(&tk)
                            .iter()
                            .filter_map(|components| components.to_str())
                            .collect();
                        if let Some(last) = tokens.last()
                            && *last == to_find
                        {
                            return Some(dir_entry.path());
                        }
                    }
                }
                _ => (), // ignore all errors for now.
            }
        }
    }
    None
}

fn load_paths() -> Vec<PathBuf> {
    let mut valid_dirs: Vec<PathBuf> = vec![];
    // dbg!(&args);
    // if !args.is_empty() {
    //     let paths = &args[0];
    //     let dirs = match paths.find('=') {
    //         Some(idx) => {
    //             let (_, dirs) = paths.split_at(idx);
    //             &dirs[1..] // split at index makes dir start with `=` byte which we do not need
    //         }
    //         None => todo!(),
    //     };

    //     if !dirs.is_empty() {
    //         let paths = env::split_paths(dirs);
    //         for p in paths {
    //             // dbg!(&p);
    //             if p.is_dir() {
    //                 valid_dirs.push(p);
    //             }
    //         }
    //     }
    // }
    match env::var_os("PATH") {
        Some(paths) => {
            for p in env::split_paths(&paths) {
                dbg!(&p);
                if p.is_dir() {
                    valid_dirs.push(p);
                }
            }
        }
        _ => (),
    }
    valid_dirs
}

fn main() {
    let _args: Vec<String> = env::args().skip(1).collect();
    match repl() {
        Ok(()) => (),
        Err(_err) => todo!(),
    }
}
