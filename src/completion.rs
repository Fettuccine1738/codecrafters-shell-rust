// use std::{collections::HashMap, rc::Rc};

use std::{env, vec};

use rustyline::{Helper, completion::Completer, highlight::Highlighter, validate::Validator};

use crate::LIST_BUILTINS;

// code taken from samples on CodeCrafter's examples page
#[derive(Debug)]
pub struct ShellHelper {
    pub exe_names: Vec<String>,
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

        // if we are not the first word, call the `ls` program and then return the
        // return the name of the matching file
        if !line[..st].trim().is_empty() {
            let completion = complete_nested_path(word);
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

        // *NOTE: Sorting here helps with deduplication but also
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

// completion in nested paths
fn complete_nested_path(path: &str) -> Vec<String> {
    let last = path.rfind("/").map_or(0, |idx| idx + 1);

    // dir will contains the first 'last' bytes, if last == 0
    // dir is empty
    let (dir, file) = path.split_at(last);

    if !dir.is_empty() && !file.is_empty() {
        return match crate::exec_program("ls", dir) {
            Ok((stdout, _)) => {
                let as_str = String::from_utf8_lossy(&stdout);
                let files: Vec<String> = as_str
                    .lines()
                    .filter(|line| *&line.starts_with(file))
                    // ls returns the exact file_names; codecrafters expects the path to the full file name
                    // as completion
                    .map(|s| format!("{dir}{s} "))
                    .collect();
                files
            }
            Err(_) => vec![],
        };
    }

    if !file.is_empty() {
        // dir is empty and we need to search in the current directory
        // hence no args passed to `ls`
        return match crate::exec_program("ls", "") {
            Ok((stdout, _)) => {
                let as_str = String::from_utf8_lossy(&stdout);
                let files: Vec<String> = as_str
                    .lines()
                    .filter(|line| *&line.starts_with(file))
                    .map(|s| format!("{s} "))
                    .collect();
                files
            }
            Err(_) => vec![],
        };
    }

    vec![]
}
