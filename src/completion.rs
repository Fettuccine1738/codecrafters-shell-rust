// use std::{collections::HashMap, rc::Rc};

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

        if !line[..st].trim().is_empty() {
            return Ok((st, vec![]));
        }

        let mut completions: Vec<String> = self
            .exe_names
            .iter()
            .map(String::as_str)
            .chain(LIST_BUILTINS.iter().copied())
            .filter(|w| w.starts_with(word))
            .map(|name| format!("{name} "))
            .collect();

        completions.sort();
        completions.dedup();

        // rings a bell to indicate there are no valid completions
        // if completions.is_empty() {
        // ! this does not work
        //     print!("\x07");
        //     // Ok((st, vec!["\x07"]))
        // }
        Ok((st, completions))
    }
}

impl rustyline::hint::Hinter for ShellHelper {
    type Hint = String;
}

impl Highlighter for ShellHelper {}
impl Validator for ShellHelper {}
impl Helper for ShellHelper {}
