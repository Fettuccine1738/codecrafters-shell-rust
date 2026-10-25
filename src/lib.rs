#[allow(unused_imports)]
use std::io::{self, Write};
use std::{
    collections::HashMap,
    env::{self},
    ffi::OsStr,
    fs::{self, OpenOptions},
    path::{Path, PathBuf},
    process::Command,
    vec,
};

use is_executable::is_executable;

static KEYWORD_EXIT_SHELL: &str = "exit";
static KEYWORD_ECHO: &str = "echo";
static KEYWORD_TYPE: &str = "type";
static KEYWORD_BUILTIN: &str = "builtin";

static OPERATOR_REDIRECT: char = '>';
// In Unix std-output  is identified by `1`.
// Usage: `echo hello 1> file.txt`
static _OPERATOR_REDIRECT_FD: &str = "1>";
static OPERATOR_APPEND_FILE: &str = ">>";

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

pub fn repl() -> anyhow::Result<()> {
    let paths = load_paths();
    // This shell is really slow when searching for executables
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

// #[derive(Default, Debug)]
// struct OuputOpts<'src> {
//     pub(crate) args: &'src str,
//     pub(crate) redirect: Option<&'src str>,
//     // true = redirect error to the file path in redirect
//     pub(crate) redirect_err: bool,
//     // append = false, means we create a new file
//     pub(crate) append: bool,
// }

/// This track destination state for writing / redirecting output
#[derive(Clone, Copy)]
enum OutputTarget<'src> {
    Stdout,
    Stderr,
    File {
        path: &'src str,
        append: bool,
        redir_err: bool,
    },
}

fn eval(
    input: &str,
    paths: &[PathBuf],
    cache: &mut HashMap<String, CacheEntry>,
) -> anyhow::Result<()> {
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

    // TODO: refactor functions to accept the struct directly
    let (args, target) = split_redirect(args);
    // this swaps the target if the File variant whose redir_err is set is matched
    let (out_target, err_target) = match target {
        // only write to a file when `2>`  is matched
        OutputTarget::File {
            path,
            append,
            redir_err,
        } => {
            if redir_err {
                (
                    OutputTarget::Stdout,
                    OutputTarget::File {
                        path,
                        append,
                        redir_err,
                    },
                )
            } else {
                (target, OutputTarget::Stderr)
            }
        }
        _ => (target, OutputTarget::Stderr),
    };

    if cmd == KEYWORD_EXIT_SHELL {
        std::process::exit(0);
    } else if cmd == KEYWORD_ECHO {
        let mut output = args.bytes().collect::<Vec<u8>>();
        if !output.is_empty() && output[output.len() - 1] != 10 {
            output.push(10u8);
        }
        // prints to terminal when redirect is set but we have no fallible impls
        // so this only creates the destination file to be redirected to and writes.
        // nothing to it.
        write_output(err_target, vec![])?;
        write_output(out_target, output)?
        // write_output(
        //     if redirect_err {
        //         output_opts.redirect
        //     } else {
        //         None
        //     },
        //     vec![],
        //     output_opts.append,
        // )?;

        // // if redirect_err is not set write to the shell
        // write_output(
        //     if !redirect_err {
        //         output_opts.redirect
        //     } else {
        //         None
        //     },
        //     output,
        //     output_opts.append,
        // )?
    } else if cmd == KEYWORD_TYPE {
        let output = type_command(args, paths, cache);
        write_output(err_target, vec![])?;
        write_output(out_target, output)?
    } else {
        // TODO: Instead of making these functions return a vector,
        // we could have a shell object that owns an output buffer
        // that can be written to and will be cleared after every repl call.
        let (out_bytes, err_bytes) = match cache.get(cmd) {
            Some(CacheEntry::Val(s)) => {
                // this program has been called before e.g /usr/bin/ls
                let prg = Path::new(s).file_name().unwrap();
                let (stdout, stderr) = exec_program(prg, args)?;
                (stdout, stderr)
            }
            Some(CacheEntry::NoVal) => {
                // again called before but not a valid program e.g /usr/bin/non-exist
                (format!("{cmd}: not found\n").into_bytes(), vec![])
            }
            _ => {
                // first time call
                if let Some(pbuf) = find_exec_file(cmd, paths) {
                    cache.insert(
                        cmd.to_string(),
                        CacheEntry::Val(pbuf.to_str().unwrap().to_string()),
                    );
                    let prg = pbuf.file_name().unwrap();
                    let (stdout, stderr) = exec_program(prg, args)?;
                    (stdout, stderr)
                } else {
                    cache.insert(cmd.to_string(), CacheEntry::NoVal);
                    // we are sending this out as error so it gets printed to eprintln
                    (vec![], format!("{cmd}: not found\n").into_bytes())
                }
            }
        };

        write_output(err_target, err_bytes)?;
        write_output(out_target, out_bytes)?
        // // if fd 2 is set we sent error to the file
        // if redirect_err {
        //     write_output(output_opts.redirect, err_bytes, output_opts.append)?
        // } else {
        //     // if an error occured we force printing out to output
        //     // because non-shell only writes non-error outputs to files when
        //     // `2>` std err isn't set.
        //     write_output(None, err_bytes, output_opts.append)?
        // }

        // if !out_bytes.is_empty() {
        //     // write to file only when `1>` is used, if `2>` we print
        //     // out .
        //     write_output(
        //         if redirect_err {
        //             None
        //         } else {
        //             output_opts.redirect
        //         },
        //         out_bytes,
        //         output_opts.append,
        //     )?
        // }
    }
    Ok(())
}

// splits args (a prompt whose command has been stripped away)
// e.g `echo hello world > foo.txt`; split_redirect(`hello .. foo.txt`)
// and returns the original string if > or 1> is not present
// or (before `(1)>`, after (`>`) )
fn split_redirect(args: &str) -> (&str, OutputTarget<'_>) {
    match args.find(OPERATOR_REDIRECT) {
        Some(pos) => {
            let (content, path) = args.split_at(pos);
            let content = if let Some(stripped) = content.strip_suffix("1") {
                stripped
            } else {
                content
            };

            // redirect error is set to true, this helps us know when to write to a file
            // versus printing out to the shell
            let mut redir_err = false;
            let content = if let Some(stripped) = content.strip_suffix("2") {
                redir_err = true;
                stripped
            } else {
                content
            };

            // trims off `>>` or `>`
            if let Some(stripped) = path.strip_prefix(OPERATOR_APPEND_FILE) {
                return (
                    content,
                    OutputTarget::File {
                        path: stripped.trim(),
                        append: true,
                        redir_err,
                    },
                );
            }

            if let Some(stripped) = path.strip_prefix(OPERATOR_REDIRECT) {
                return (
                    content,
                    OutputTarget::File {
                        path: stripped.trim(),
                        append: false,
                        redir_err,
                    },
                );
            };

            (content, OutputTarget::Stdout)
        }
        // returns args because the redirect operator is missing and we
        None => (args.trim(), OutputTarget::Stdout),
    }
}

fn write_output(target: OutputTarget<'_>, bytes: Vec<u8>) -> anyhow::Result<()> {
    match target {
        OutputTarget::Stdout => {
            if !bytes.is_empty() {
                print!("{}", String::from_utf8_lossy(&bytes));
            }
        }
        OutputTarget::Stderr => {
            if !bytes.is_empty() {
                eprint!("{}", String::from_utf8_lossy(&bytes));
            }
        }
        OutputTarget::File { path, append, .. } => {
            let mut opts = OpenOptions::new();
            opts.create(true);

            if append {
                opts.append(true);
            } else {
                opts.write(true).truncate(true);
            }

            let mut file = opts.open(path)?;
            file.write_all(&bytes)?;
        }
    }

    Ok(())
}

// fn write_output(path: Option<&str>, bytes: Vec<u8>, append_file: bool) -> anyhow::Result<()> {
//     // ascii '\n' = 10u8
//     match path {
//         Some(path) => {
//             // this should create a new file if it does not exist
//             let mut oo = OpenOptions::new();
//             if append_file {
//                 oo.append(true);
//             } else {
//                 oo.truncate(true);
//                 oo.write(true);
//             }

//             let mut file = oo.create(true).open(path)?;

//             file.write_all(&bytes)?;
//             // let ends_new_line = !bytes.is_empty() && bytes[bytes.len() - 1] == 10u8;
//             // if !ends_new_line {
//             //     file.write_all(&[10u8])?;
//             // }
//         }
//         None => {
//             // avoids printing to the std out when we don't need to
//             if !bytes.is_empty() {
//                 let ends_new_line = bytes[bytes.len() - 1] == 10u8;
//                 let as_text = unsafe { String::from_utf8_unchecked(bytes) };
//                 if ends_new_line {
//                     print!("{}", as_text);
//                 } else {
//                     println!("{}", as_text);
//                 }
//             }
//         }
//     }
//     Ok(())
// }

fn type_command(cmd: &str, paths: &[PathBuf], cache: &mut HashMap<String, CacheEntry>) -> Vec<u8> {
    if find_in_cmds(cmd) {
        return format!("{cmd} is a shell builtin\n").into_bytes();
    }

    if let Some(entry) = cache.get(cmd) {
        return match entry {
            CacheEntry::Val(path) => format!("{cmd} is {path}\n").into_bytes(),
            CacheEntry::NoVal => format!("{cmd}: not found\n").into_bytes(),
        };
    }

    match find_exec_file(cmd, paths) {
        Some(pathbuf) => {
            let path = pathbuf.to_str().unwrap();
            cache.insert(cmd.to_string(), CacheEntry::Val(path.to_string()));
            format!("{cmd} is {path}\n").into_bytes()
        }
        None => {
            cache.insert(cmd.to_string(), CacheEntry::NoVal);
            format!("{cmd}: not found\n").into_bytes()
        }
    }
}

fn exec_program<T: AsRef<OsStr>>(program: T, args: &str) -> anyhow::Result<(Vec<u8>, Vec<u8>)> {
    let args: Vec<&str> = args.split_whitespace().collect();
    // spawn a new process that inherits our shell's environment
    // and current working directory
    let output = Command::new(program)
        .args(args)
        .output()
        .expect("Process should execute successfully");
    Ok((output.stdout, output.stderr))
}

fn find_in_cmds(to_find: &str) -> bool {
    for s in LIST_VALID_CMDS {
        if s == to_find {
            return true;
        }
    }
    false
}

// traverses the directories in paths for executable files whose file_name
// matches to_find
fn find_exec_file(to_find: &str, paths: &[PathBuf]) -> Option<PathBuf> {
    for path in paths {
        let dir = fs::read_dir(path).unwrap();
        for entry in dir.flatten() {
            if is_executable(entry.path()) && entry.file_name().into_string().unwrap() == to_find {
                return Some(entry.path());
            }
        }
    }
    None
}

// called at the start of our shell program to load the enviroment variable
// paths. This is where executables etc are searched
fn load_paths() -> Vec<PathBuf> {
    let mut valid_dirs: Vec<PathBuf> = vec![];
    if let Some(paths) = env::var_os("PATH") {
        for p in env::split_paths(&paths) {
            // do we need this? the paths here are supposed to be directories
            if p.is_dir() {
                valid_dirs.push(p);
            }
        }
    }
    valid_dirs
}
