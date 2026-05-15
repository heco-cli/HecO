use crate::output;
use indicatif::{ProgressBar, ProgressDrawTarget, ProgressStyle};
use owo_colors::OwoColorize;
use std::cell::RefCell;
use std::io::IsTerminal;

pub struct StatusBar {
    bar: ProgressBar,
    progress_enabled: bool,
    state: RefCell<State>,
}

struct State {
    label: String,
    total: usize,
    current: usize,
}

impl StatusBar {
    pub fn new(label: &str, total: usize) -> Self {
        let progress_enabled = !crate::output::is_quiet()
            && std::io::stderr().is_terminal()
            && std::env::var("TERM")
                .map(|term| term != "dumb")
                .unwrap_or(true)
            && std::env::var_os("CI").is_none();

        let bar = ProgressBar::new(total as u64);
        if progress_enabled {
            bar.set_draw_target(ProgressDrawTarget::stderr());
            bar.set_style(
                ProgressStyle::with_template(
                    "{prefix:>12} [{bar:24.green/black}] {pos}/{len}: {msg}",
                )
                .unwrap(),
            );
            bar.set_prefix(label.to_string());
        } else {
            bar.set_draw_target(ProgressDrawTarget::hidden());
        }

        Self {
            bar,
            progress_enabled,
            state: RefCell::new(State {
                label: label.to_string(),
                total,
                current: 0,
            }),
        }
    }

    #[allow(dead_code)]
    pub fn set_total(&self, total: usize) {
        let mut state = self.state.borrow_mut();
        state.total = total;
        self.bar.set_length(total as u64);
    }

    pub fn println(&self, content: &str) {
        if crate::output::is_quiet() {
            return;
        }

        let content = maybe_strip_ansi(content);
        if self.progress_enabled {
            self.bar.suspend(|| output::line(&content));
        } else {
            output::line(content);
        }
    }

    pub fn status(&self, verb: &str, description: &str) {
        if crate::output::is_quiet() {
            return;
        }

        let content = format!(
            "{:>width$} {}",
            verb.green().bold(),
            description,
            width = output::STATUS_WIDTH
        );
        self.println(&content);
    }

    pub fn set_progress(&self, current: usize, message: &str) {
        if crate::output::is_quiet() || !self.progress_enabled {
            return;
        }

        let mut state = self.state.borrow_mut();
        let position = current.min(state.total);
        state.current = position;
        self.bar.set_prefix(state.label.clone());
        self.bar.set_position(position as u64);
        self.bar.set_message(maybe_strip_ansi(message));
        self.bar.tick();
    }

    pub fn finish_and_clear(&self) {
        if crate::output::is_quiet() || !self.progress_enabled {
            return;
        }

        self.bar.finish_and_clear();
    }

    pub fn task(&self, name: &str, description: &str) -> TaskGuard<'_> {
        self.begin_task(name, description);
        TaskGuard { _bar: self }
    }

    fn begin_task(&self, name: &str, description: &str) {
        if crate::output::is_quiet() {
            return;
        }

        let mut state = self.state.borrow_mut();
        state.current += 1;

        if self.progress_enabled {
            self.bar.set_prefix(state.label.clone());
            self.bar.set_position(state.current as u64);
            self.bar
                .set_message(maybe_strip_ansi(&format!("{name} {description}")));
            self.bar.tick();
        } else {
            output::status(name, description);
        }
    }

    pub fn finish_with_message(&self, msg: &str) {
        if crate::output::is_quiet() {
            return;
        }

        if self.progress_enabled {
            self.bar.finish_and_clear();
        }
        output::line(maybe_strip_ansi(msg));
    }
}

fn maybe_strip_ansi(content: &str) -> String {
    if std::env::var("NO_COLOR").is_ok() {
        anstream::adapter::strip_str(content).to_string()
    } else {
        content.to_string()
    }
}

pub struct TaskGuard<'a> {
    _bar: &'a StatusBar,
}

impl<'a> Drop for TaskGuard<'a> {
    fn drop(&mut self) {}
}
