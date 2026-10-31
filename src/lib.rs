#[allow(unused_imports)]
use std::io::{self, Write};
use std::{
    cell::RefCell,
    collections::HashMap,
    env::{self},
    ffi::OsStr,
    fs::{self, OpenOptions},
    path::{Path, PathBuf},
    process::{Child, Command, ExitStatus},
    rc::Rc,
    vec,
};

mod completion;

use is_executable::is_executable;
use rustyline::{
    CompletionType, Editor,
    config::{BellStyle, Configurer},
    history::DefaultHistory,
};

use crate::completion::ShellHelper;

static BUILTIN_EXIT_SHELL: &str = "exit";
static BUILTIN_ECHO: &str = "echo";
static BUILTIN_TYPE: &str = "type";
static BUILTIN_BUILTIN: &str = "builtin";
static BUILTIN_COMPLETE: &str = "complete";
static BUILTIN_JOBS: &str = "jobs";

static OPERATOR_REDIRECT: char = '>';
// In Unix std-output  is identified by `1`.
// Usage: `echo hello 1> file.txt`
static _OPERATOR_REDIRECT_FD: &str = "1>";
static OPERATOR_APPEND_FILE: &str = ">>";
static OPERATOR_AMPS_JOBS: &str = "&";

static STATUS_JOB_DONE: &str = "Done";
static STATUS_JOB_RUNNING: &str = "running";

static LN_WIDTH_JOB_STATUS: usize = 24;

static LIST_BUILTINS: [&str; 6] = [
    BUILTIN_BUILTIN,
    BUILTIN_ECHO,
    BUILTIN_EXIT_SHELL,
    BUILTIN_TYPE,
    BUILTIN_COMPLETE,
    BUILTIN_JOBS,
];

struct JobInfo {
    // starts at 1, increments for each new job;
    job_id: u32,
    // process id
    // p_id: u32, unused, left out for now.
    cmd_string: String,
    // handle to the child process, to check the status codes
    handle_child: Child,
    // caches whether this jobs already exited, i.e handl_child.is_some()
    exited: bool,
}

// type Cache = Mutex<LazyLock<HashMap<String, String>>>;

// static EXECUTABLES_CACHE: Cache = Mutex::new(LazyLock::new(|| HashMap::new()));

pub fn repl() -> anyhow::Result<()> {
    // maps ls -> /usr/bin/ls
    let executables = load_executables();
    let exe_names = executables.keys().cloned().collect();
    // a stack of job ids and pid.
    let mut jobs_stack: Vec<JobInfo> = vec![];

    // cache to store completions registered with the `BUILTIN_COMPLETE`
    // Interior mutablity is needed here, so the `ShellHelper` sees the updated
    // scripts.
    let completion_cache: Rc<RefCell<HashMap<String, String>>> =
        Rc::new(RefCell::new(HashMap::new()));
    let mut editor: Editor<ShellHelper, DefaultHistory> = Editor::new()?;
    editor.set_helper(Some(ShellHelper {
        exe_names,
        completion_scripts: Rc::clone(&completion_cache),
    }));
    editor.set_completion_type(CompletionType::List);
    editor.set_bell_style(BellStyle::Audible);

    editor.set_cursor_visibility(true)?;

    loop {
        let input = editor.readline("$ ")?;
        let _ = eval(
            &input,
            &executables,
            &mut completion_cache.borrow_mut(),
            &mut jobs_stack,
        );
    }
}

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
    executables: &HashMap<String, String>,
    completion_cache: &mut HashMap<String, String>,
    jobs_stack: &mut Vec<JobInfo>,
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

    if cmd == BUILTIN_EXIT_SHELL {
        std::process::exit(0);
    } else if cmd == BUILTIN_ECHO {
        let mut output = args.bytes().collect::<Vec<u8>>();
        if !output.is_empty() && output[output.len() - 1] != 10 {
            output.push(10u8);
        }
        // prints to terminal when redirect is set but we have no fallible impls
        // so this only creates the destination file to be redirected to and writes.
        // nothing to it.
        write_output(err_target, vec![])?;
        write_output(out_target, output)?
    } else if cmd == BUILTIN_TYPE {
        if args.is_empty() {
            return Ok(());
        }

        let output = type_command(args, executables);
        write_output(err_target, vec![])?;
        write_output(out_target, output)?
    } else if cmd == BUILTIN_COMPLETE {
        let args: Vec<&str> = args.split_whitespace().collect();
        if args.len() < 2 {
            todo!()
        }

        let flag = args[0];

        if flag == "-p" {
            match completion_cache.get(args[1]) {
                Some(c) => {
                    // prints complete -C 'path/to/completer' registered_cmd
                    let msg = format!("{BUILTIN_COMPLETE} -C '{c}' {}\n", args[1]);
                    write_output(out_target, msg.into_bytes())?
                }
                None => {
                    let msg = format!(
                        "{BUILTIN_COMPLETE}: {}: no completion specification\n",
                        args[1]
                    );
                    write_output(err_target, msg.into_bytes())?
                }
            }
        } else if flag == "-C" {
            let completion = args[1].to_string();
            let command = args[2].to_string();

            completion_cache.insert(command, completion);
        } else if flag == "-r" {
            let command = args[1].to_string();
            let _ = completion_cache.remove(&command);
        }
    } else if cmd == BUILTIN_JOBS {
        let jobs_len = jobs_stack.len();
        for idx in 0..jobs_len {
            let job = &mut jobs_stack[idx];
            let (status, exited) = match job.handle_child.try_wait()? {
                Some(_) => (STATUS_JOB_DONE, true),
                None => (STATUS_JOB_RUNNING, false),
            };
            job.exited = exited;

            if idx == jobs_len - 1 {
                write_output(
                    out_target,
                    format!(
                        "[{}]+  {:<LN_WIDTH_JOB_STATUS$} {}\n",
                        job.job_id, status, job.cmd_string
                    )
                    .into_bytes(),
                )?;
            } else if idx == jobs_len - 2 {
                write_output(
                    out_target,
                    format!(
                        "[{}]-  {:<LN_WIDTH_JOB_STATUS$} {}\n",
                        job.job_id, status, job.cmd_string
                    )
                    .into_bytes(),
                )?;
            } else {
                write_output(
                    out_target,
                    format!(
                        "[{}]  {:<LN_WIDTH_JOB_STATUS$} {}\n",
                        job.job_id, status, job.cmd_string
                    )
                    .into_bytes(),
                )?;
            }
        }

        // accumulate finished job's index
        let finished: Vec<usize> = jobs_stack
            .iter()
            .enumerate()
            .filter_map(|(index, job)| job.exited.then_some(index))
            .collect();

        for index in finished.into_iter().rev() {
            jobs_stack.remove(index);
        }
    } else {
        let program = cmd;
        // strip off background job operator `&`   if present
        let mut run_background = false;
        let stripped_args = if args.ends_with(OPERATOR_AMPS_JOBS) {
            run_background = true;
            args.strip_suffix(OPERATOR_AMPS_JOBS).unwrap()
        } else {
            args
        };

        if run_background {
            let child = exec_program_no_wait(program, stripped_args)?;
            let ch_pid = child.id();
            let jid = if jobs_stack.is_empty() {
                1
            } else {
                // jobs_stack.binary_search(x)
                jobs_stack.last().unwrap().job_id + 1
            };

            write_output(out_target, format!("[{}] {ch_pid}\n", jid).into_bytes())?;

            // push onto stack
            jobs_stack.push(JobInfo {
                job_id: jid,
                // p_id: ch_pid,
                cmd_string: {
                    // clean up the arguments
                    let mut run_command = program.to_string();
                    // bulid the arsgs cleanly
                    let cl_args: Vec<&str> = args.split_whitespace().collect();
                    let new_args = cl_args.join(" ");
                    run_command.push(' ');
                    run_command.push_str(&new_args);
                    run_command
                },
                handle_child: child,
                exited: false,
            });

            return Ok(());
        }

        // TODO: Instead of making these functions return a vector,
        // we could have a shell object that owns an output buffer
        // that can be written to and will be cleared after every repl call.
        let (out_bytes, err_bytes) = match executables.get(program) {
            // Some(CacheEntry::Val(s)) => {
            Some(_) => {
                // this program has been called before e.g /usr/bin/ls
                // let prg = Path::new(s).file_name().unwrap();
                // not cmd is the programs alias so this is fine
                let (stdout, stderr) = exec_program(program, stripped_args)?;
                (stdout, stderr)
            }
            // Some(CacheEntry::NoVal) => {
            None => {
                // again called before but not a valid program e.g /usr/bin/non-exist
                (format!("{program}: not found\n").into_bytes(), vec![])
            }
        };

        write_output(err_target, err_bytes)?;
        write_output(out_target, out_bytes)?
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
                io::stdout().write_all(&bytes)?;
                io::stdout().flush()?;
            }
        }
        OutputTarget::Stderr => {
            if !bytes.is_empty() {
                io::stderr().write_all(&bytes)?;
                io::stderr().flush()?;
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

fn type_command(cmd: &str, executables: &HashMap<String, String>) -> Vec<u8> {
    if find_in_cmds(cmd) {
        return format!("{cmd} is a shell builtin\n").into_bytes();
    }
    // if let Some(entry) = cache.get(cmd) {
    //     return match entry {
    //         CacheEntry::Val(path) => format!("{cmd} is {path}\n").into_bytes(),
    //         CacheEntry::NoVal => format!("{cmd}: not found\n").into_bytes(),
    //     };
    // }

    match executables.get(cmd) {
        Some(path) => {
            // cache.insert(cmd.to_string(), CacheEntry::Val(path.to_string()));
            format!("{cmd} is {path}\n").into_bytes()
        }
        None => {
            // cache.insert(cmd.to_string(), CacheEntry::NoVal);
            format!("{cmd}: not found\n").into_bytes()
        }
    }
}

fn exec_program_with_env<T: AsRef<OsStr>>(
    program: T,
    args: Vec<&str>,
    env_vars: &HashMap<&'static str, String>,
) -> anyhow::Result<(Vec<u8>, Vec<u8>)> {
    // let args: Vec<&str> = args.split_whitespace().collect();
    // spawn a new process that inherits our shell's environment
    // and current working directory
    let output = Command::new(program)
        .args(args)
        .envs(env_vars)
        .output()
        .expect("Process should execute successfully");
    Ok((output.stdout, output.stderr))
}

fn exec_program_no_wait<T: AsRef<OsStr>>(program: T, args: &str) -> anyhow::Result<Child> {
    let args: Vec<&str> = args.split_whitespace().collect();
    // spawn a new process, without waiting for completions don't need the child
    let child: std::process::Child = Command::new(program)
        .args(args)
        .spawn()
        .expect("Process should execute successfully");
    Ok(child)
}

fn exec_program<T: AsRef<OsStr>>(program: T, args: &str) -> anyhow::Result<(Vec<u8>, Vec<u8>)> {
    let args: Vec<&str> = args.split_whitespace().collect();
    // spawn a new process that inherits our shell's environment
    // and current working directory
    let output = Command::new(program)
        .args(args)
        // .status()
        .output()
        .expect("Process should execute successfully");
    Ok((output.stdout, output.stderr))
}

fn find_in_cmds(to_find: &str) -> bool {
    for s in LIST_BUILTINS {
        if s == to_find {
            return true;
        }
    }
    false
}

fn load_executables() -> HashMap<String, String> {
    let mut map: HashMap<String, String> = HashMap::new();

    for path in load_paths() {
        let dir = fs::read_dir(path).unwrap();
        for entry in dir.flatten() {
            if is_executable(entry.path()) {
                let Ok(name) = entry.file_name().into_string() else {
                    continue;
                };

                let Some(other) = entry.path().to_str().map(str::to_owned) else {
                    continue;
                };
                map.entry(name).or_insert(other);
            }
        }
    }

    map
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

// f is a predicate that tests whether the path matches
fn visit_dirs(
    dir: &Path,
    collection: &mut Vec<String>,
    f: &dyn Fn(&Path, &mut Vec<String>),
    // supplier: &mut dyn FnMut(&Path) -> String,
    recurse: bool,
) -> io::Result<()> {
    if dir.is_dir() {
        for entry in fs::read_dir(dir)? {
            let entry = entry?;
            let path = entry.path();

            f(&path, collection);

            if recurse && path.is_dir() {
                visit_dirs(&path, collection, f, recurse)?;
            }
        }
    }
    Ok(())
}
