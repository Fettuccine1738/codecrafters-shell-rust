use std::{
    cell::RefCell, collections::HashMap, env, hash::Hash, net::ToSocketAddrs, path::Path, rc::Rc,
    vec,
};

use rustyline::{Helper, completion::Completer, highlight::Highlighter, validate::Validator};

use crate::LIST_BUILTINS;

static COMP_LINE: &'static str = "COMP_LINE";
static COMP_POINT: &'static str = "COMP_POINT";
// code taken from samples on CodeCrafter's examples page
#[derive(Debug)]
pub struct ShellHelper {
    pub exe_names: Vec<String>,
    pub completion_scripts: Rc<RefCell<HashMap<String, String>>>,
}

impl Completer for ShellHelper {
    type Candidate = String;

    fn complete(
        &self, // FIXME should be `&mut self`
        line: &str,
        pos: usize,
        _ctx: &rustyline::Context<'_>,
    ) -> rustyline::Result<(usize, Vec<Self::Candidate>)> {
        // looks at the text before the cursor finds the last space
        // and sets the `st` index right after it (or at 0 if no space).
        let st = line[..pos].rfind(" ").map_or(0, |i| i + 1);
        let word = &line[st..pos];
        let before_word = line[..st].trim();

        // if there is text before the word  we found, we look for completion scripts first
        // then programs next
        if !before_word.is_empty() {
            let tokens: Vec<&str> = line.split_whitespace().collect();
            // we only complete if the word is the first and only word
            // this way we avoid completing a matching word that is embedded in some line
            // e.g `$ ls 'word_that_matches_completion'`
            // ! WARN: Previously we checked that the registered command was the only token
            // ! on the line. This way we didn't complete the same word used in another context.
            // ! cc wants to pass arguments to the completer_script and that requires us to match on only
            // ! the first token.

            // let is_script = match tokens {
            //     Some(s) => {
            //         if s == before_word {
            //             true
            //         } else {
            //             false
            //         }
            //     }
            //     _ => false,
            // };
            // ! Bugbug: what
            let cmd = tokens[0]; // command the script was registered for
            if let Some(script) = self.completion_scripts.borrow().get(cmd) {
                // reconstructing args for the
                let mut args = cmd.to_string();
                args.push_str(" ");
                if tokens.len() > 1 {
                    args.push_str(tokens.last().unwrap()); // word currently being completed
                    args.push_str(" ");
                }
                if tokens.len() == 3 {
                    args.push_str(tokens[1]); // word immediately bofore the word being completed 
                } else {
                    // if there is no immediate word before the word to be completed
                    // we pass in an empty string.
                    // * how useful, if args in exec_program splits on whitespace
                    args.push_str(" ");
                };
                // completion environment variables
                let mut env_vars: HashMap<&'static str, String> = HashMap::new();
                env_vars.insert(COMP_LINE, line.to_string());
                env_vars.insert(COMP_POINT, pos.to_string());

                let (s_out, _) = crate::exec_program_with_env(script, &args, &env_vars).unwrap();
                // each line is a completion candidiate so we split the string into candidates
                let completions = String::from_utf8(s_out)
                    .unwrap()
                    .lines()
                    .map(|line| format!("{line} "))
                    .collect();
                return Ok((st, completions));
            }
            let mut completion = complete_nested_paths(word);
            completion.sort();
            return Ok((st, completion));
        }

        let mut completions: Vec<String> = self
            .exe_names
            .iter()
            .map(String::as_str)
            .chain(LIST_BUILTINS.iter().copied())
            .filter(|w| w.starts_with(word))
            .map(|name| format!("{name} "))
            .collect();

        // * NOTE: Sorting here helps with deduplication but also
        // * with matching the longest common prefix
        completions.sort();
        completions.dedup();

        Ok((st, completions))
    }
}

impl rustyline::hint::Hinter for ShellHelper {
    type Hint = String;
}

impl Highlighter for ShellHelper {}
impl Validator for ShellHelper {}
impl Helper for ShellHelper {}

// fn complete_directory(path: &str) -> Vec<String> {}

fn complete_nested_paths(path: &str) -> Vec<String> {
    let last = path.rfind("/").map_or(0, |idx| idx + 1);

    // dir will contains the first 'last' bytes, if last == 0
    // dir is empty
    let (dir, file) = path.split_at(last);
    let mut res: Vec<String> = vec![];

    if !dir.is_empty() && !file.is_empty() {
        let cur_dir = Path::new(dir);
        let _ = crate::visit_dirs(
            cur_dir,
            &mut res,
            &|path, res| {
                let s = path.file_name().unwrap().to_str().unwrap();
                if s.starts_with(file) {
                    // here we concatenate to get the full dir back
                    let s = if path.is_dir() {
                        format!("{dir}{s}/")
                    } else {
                        format!("{dir}{s} ")
                    };

                    res.push(s);
                }
            },
            false,
        );

        // if !res.is_empty() {
        // }
        return res;
    }

    // matches on foo/bar/ -> there is no file to match so we match all files in this dir
    if !dir.is_empty() {
        let cur_dir = Path::new(dir);
        let _ = crate::visit_dirs(
            cur_dir,
            &mut res,
            &|path, res| {
                let s = path.file_name().unwrap().to_str().unwrap();
                // there is no file to match on, so we send all the files/dirs out as completions
                let s = if path.is_dir() {
                    format!("{dir}{s}/") // append dir
                } else {
                    format!("{dir}{s} ")
                };

                res.push(s);
            },
            false,
        );
        return res;
    }

    // search in the current directory, target might be a file or a
    // directory, file may or may not be empty
    // regardless search the cwd
    // dir is empty and we need to search in the current directory
    // hence no args passed to `ls`
    // TODO: Fix unwrap and handle err variant
    let cur_dir = env::current_dir().unwrap();
    let _ = crate::visit_dirs(
        &cur_dir,
        &mut res,
        &|path, res| {
            let s = path.file_name().unwrap().to_str().unwrap();
            if s.starts_with(file) {
                let s = if path.is_dir() {
                    format!("{s}/")
                } else {
                    format!("{s} ")
                };

                res.push(s);
            }
        },
        false,
    );
    // TODO: map names to expecsted output
    res
}

// #[cfg(test)]
// mod tests {
//     use super::script_output_to_completions;

//     #[test]
//     fn script_output_becomes_one_spaced_completion_per_line() {
//         assert_eq!(
//             script_output_to_completions(b"first\nsecond\n"),
//             vec!["first ", "second "]
//         );
//     }

//     #[test]
//     fn empty_script_output_has_no_completions() {
//         assert!(script_output_to_completions(b"").is_empty());
//     }
// }
