#[allow(unused_imports)]
use std::io::{self, Write};
use std::{
    collections::HashMap,
    env::{self},
    fs::{self},
    path::{Path, PathBuf},
    process::Command,
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

// type Cache = Mutex<LazyLock<HashMap<String, String>>>;

// static EXECUTABLES_CACHE: Cache = Mutex::new(LazyLock::new(|| HashMap::new()));
pub enum CacheEntry {
    Val(String),
    NoVal,
}

pub fn repl() -> Result<(), io::Error> {
    let paths = load_paths();
    // This shell is really slow when it comes to searching for executables
    // we store previously searched executables here before looking up in the
    // `PATH` env directories.
    // Usage: `ls -a` will have called the find_exec_file(ls) and will return
    // /usr/bin/ls
    // `ls -a` just needs to hit the cache and no second lookup.
    let mut cache: HashMap<String, CacheEntry> = HashMap::new();
    loop {
        print!("$ ");
        io::stdout().flush().unwrap();
        let mut buf = String::new();
        io::stdin().read_line(&mut buf)?;
        let buf = buf.trim();

        let _ = eval(buf, &paths, &mut cache);
    }
}

pub struct EvalError;

fn eval(
    input: &str,
    paths: &[PathBuf],
    cache: &mut HashMap<String, CacheEntry>,
) -> Result<(), EvalError> {
    let input = input.trim();
    if input.is_empty() {
        return Ok(());
    }

    let (cmd, args) = match input.find(|ch: char| ch.is_whitespace()) {
        Some(index) => {
            let (k, r) = input.split_at(index);
            (k, r.trim())
        }
        None => (input, ""),
    };

    if cmd == KEYWORD_EXIT_SHELL {
        std::process::exit(0);
    } else if cmd == KEYWORD_ECHO {
        println!("{args}");
    } else if cmd == KEYWORD_TYPE {
        let cmd = args;
        if find_in_cmds(cmd) {
            println!("{cmd} is a shell builtin");
            return Ok(());
        }

        if cache.contains_key(cmd) {
            let v = cache.get(cmd).unwrap();
            match v {
                CacheEntry::Val(v) => {
                    println!("{cmd} is {v}");
                }
                CacheEntry::NoVal => {
                    println!("{cmd}: not found");
                }
            }
        }

        match find_exec_file(cmd, paths) {
            Some(pathbuf) => {
                let v = pathbuf.to_str().unwrap();
                println!("{cmd} is {v}");
                cache.insert(cmd.to_string(), CacheEntry::Val(v.to_string()));
            }
            None => {
                println!("{cmd}: not found");
                cache.insert(cmd.to_string(), CacheEntry::NoVal);
            }
        }
        return Ok(());
    } else {
        if cache.contains_key(cmd) {
            let v = cache.get(cmd).unwrap();
            match v {
                CacheEntry::Val(v) => {
                    println!("{cmd} is {v}");
                }
                CacheEntry::NoVal => {
                    println!("{cmd}: not found");
                }
            }
            return Ok(());
        } else if let Some(pb) = find_exec_file(cmd, paths) {
            // see if an executable with matching name exists
            cache.insert(
                cmd.to_string(),
                CacheEntry::Val(pb.to_str().unwrap().to_string()),
            );
            let program = pb.to_str().unwrap();
            exec_program(program, args);
        }
        cache.insert(cmd.to_string(), CacheEntry::NoVal);
        println!("{cmd}: not found");
    }

    Ok(())
}

fn exec_program(program: &str, args: &str) {
    let args: Vec<&str> = args.split_whitespace().collect();
    // spawn a new process that inherits our shell's environment
    // and current working directory
    let output = Command::new(program)
        .args(args)
        .output()
        .expect("Process should execute successfully");

    // println!("status: {}", output.status);
    let _ = io::stdout().write_all(&output.stdout);
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
            if let Ok(dir_entry) = entry {
                let tk = dir_entry.file_name().into_string().unwrap();
                if is_executable(dir_entry.path()) {
                    // split the path on the OS's delimiter
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
        }
    }
    None
}

fn load_paths() -> Vec<PathBuf> {
    let mut valid_dirs: Vec<PathBuf> = vec![];
    if let Some(paths) = env::var_os("PATH") {
        for p in env::split_paths(&paths) {
            if p.is_dir() {
                valid_dirs.push(p);
            }
        }
    }
    valid_dirs
}
