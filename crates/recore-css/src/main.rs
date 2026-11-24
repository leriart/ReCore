//! Reads each Discord CSS theme named on the command line and prints what the
//! interpreter found, for checking a theme by hand.

use std::path::{Path, PathBuf};
use std::process::ExitCode;

use recore_css::{Mode, Options, ThemeFile};

const USAGE: &str = "\
Usage: recore-css [OPTIONS] <PATH>...

Reads Discord CSS themes (`.css` or `.theme.css` files) and prints the id,
name, palette, look, options and fonts the interpreter found in each one.

Arguments:
  <PATH>...       One or more theme files to read.

Options:
      --dark      Read the themes as dark, forcing `Mode::Dark`.
      --light     Read the themes as light, forcing `Mode::Light`.
                  The last of --dark and --light wins.
  -h, --help      Print this help and exit.
  -V, --version   Print the interpreter version and exit.

Without --dark or --light the themes are read in `Mode::Auto`, which lets
their own root colours decide. Warnings go to stderr as `warning: ` lines.
Every file is read even when one fails; the exit code is 1 when any file
could not be read or parsed, 2 for a usage error, and 0 otherwise.
";

/// What the command line asks for.
enum Action {
    Help,
    Version,
    Run { mode: Mode, paths: Vec<PathBuf> },
}

fn main() -> ExitCode {
    let action = match parse_args(std::env::args().skip(1)) {
        Ok(action) => action,
        Err(error) => {
            eprintln!("error: {error}");
            eprintln!("try `recore-css --help` for usage");
            return ExitCode::from(2);
        }
    };

    match action {
        Action::Help => {
            print!("{USAGE}");
            ExitCode::SUCCESS
        }
        Action::Version => {
            println!("{}", recore_css::VERSION);
            ExitCode::SUCCESS
        }
        Action::Run { mode, paths } => run(mode, &paths),
    }
}

/// Reads the arguments: `--help`, `--version`, the mode flags and the theme
/// paths.
fn parse_args(args: impl Iterator<Item = String>) -> Result<Action, String> {
    let mut mode = Mode::Auto;
    let mut paths = Vec::new();
    let mut positional_only = false;

    for arg in args {
        if positional_only {
            paths.push(PathBuf::from(arg));
            continue;
        }
        match arg.as_str() {
            "-h" | "--help" => return Ok(Action::Help),
            "-V" | "--version" => return Ok(Action::Version),
            "--" => positional_only = true,
            "--dark" => mode = Mode::Dark,
            "--light" => mode = Mode::Light,
            other if other.starts_with('-') && other != "-" => {
                return Err(format!("unknown option `{other}`"));
            }
            _ => paths.push(PathBuf::from(arg)),
        }
    }

    if paths.is_empty() {
        return Err("no theme files given".to_string());
    }
    Ok(Action::Run { mode, paths })
}

/// Reads every path, printing each theme and its warnings, and reports
/// whether any of them failed.
fn run(mode: Mode, paths: &[PathBuf]) -> ExitCode {
    let options = Options::with_mode(mode);
    let mut failed = false;

    for path in paths {
        match recore_css::load_with(path, &options) {
            Ok(theme) => print_theme(path, &theme),
            Err(error) => {
                failed = true;
                // The loader's read error already names the file; add the path
                // only when one of its errors does not.
                let shown = path.display().to_string();
                if error.contains(&shown) {
                    eprintln!("error: {error}");
                } else {
                    eprintln!("error: {shown}: {error}");
                }
            }
        }
    }

    if failed {
        ExitCode::from(1)
    } else {
        ExitCode::SUCCESS
    }
}

/// Prints one theme's header and fields, then its warnings.
fn print_theme(path: &Path, theme: &ThemeFile) {
    println!("=== {}", path.display());
    println!("id: {}", theme.id);
    println!("name: {}", theme.name);
    println!("dark: {}", theme.palette.dark);
    println!("chat: {}", theme.palette.chat.hex());
    println!("sidebar: {}", theme.palette.sidebar.hex());
    println!("rail: {}", theme.palette.rail.hex());
    println!("text: {}", theme.palette.text.hex());
    println!("accent: {}", theme.palette.accent.hex());
    println!("radius: {:?}", theme.look.radius);
    println!("font: {:?}", theme.look.font_family);
    println!("options: {}", theme.options.len());
    println!("fonts: {}", theme.fonts.len());
    for warning in &theme.warnings {
        eprintln!("warning: {warning}");
    }
}
