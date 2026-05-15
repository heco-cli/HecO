use anstream::{eprintln, println};
use owo_colors::OwoColorize;
use std::fmt::Display;
use std::sync::atomic::{AtomicBool, AtomicU8, Ordering};
use std::time::Duration;

pub const STATUS_WIDTH: usize = 12;

static QUIET: AtomicBool = AtomicBool::new(false);
static VERBOSE: AtomicU8 = AtomicU8::new(0);

pub fn set_quiet(quiet: bool) {
    QUIET.store(quiet, Ordering::Relaxed);
}

pub fn is_quiet() -> bool {
    QUIET.load(Ordering::Relaxed)
}

pub fn set_verbose(level: u8) {
    VERBOSE.store(level, Ordering::Relaxed);
}

pub fn verbose_level() -> u8 {
    VERBOSE.load(Ordering::Relaxed)
}

pub fn status(verb: &str, message: impl Display) {
    if is_quiet() {
        return;
    }

    eprintln!(
        "{:>width$} {}",
        verb.green().bold(),
        message,
        width = STATUS_WIDTH
    );
}

pub fn finished(message: &str, elapsed: Duration) {
    if message.trim().is_empty() {
        status("Finished", format_args!("in {:.2?}", elapsed));
    } else {
        status("Finished", format_args!("{} in {:.2?}", message, elapsed));
    }
}

pub fn warning(message: impl Display) {
    eprintln!("{}: {}", "warning".yellow().bold(), message);
}

pub fn error(message: impl Display) {
    eprintln!("{}: {}", "error".red().bold(), message);
}

pub fn line(message: impl Display) {
    eprintln!("{}", message);
}

pub fn stdout_line(message: impl Display) {
    println!("{}", message);
}
