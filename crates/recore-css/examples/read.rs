//! Reads each `.css` file named on the command line and prints what the
//! interpreter found, for checking a theme by hand, and every rule or
//! declaration it left out with the line it sits on.

fn main() {
    let mut failed = false;
    let options = recore_css::Options {
        diagnostics: true,
        ..recore_css::Options::default()
    };
    for path in std::env::args().skip(1) {
        match recore_css::load_with(&path, &options) {
            Ok(theme) => {
                println!("=== {}", path);
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
                println!("native: {:?}", theme.native);
                for warning in &theme.warnings {
                    println!("warning: {warning}");
                }
                for diagnostic in &theme.diagnostics {
                    let severity = match diagnostic.severity {
                        recore_css::Severity::Dropped => "dropped",
                        recore_css::Severity::Skipped => "skipped",
                        recore_css::Severity::Warning => "warning",
                    };
                    if diagnostic.line > 0 {
                        println!(
                            "{severity}:{}:{}: {}",
                            diagnostic.line, diagnostic.column, diagnostic.message
                        );
                    } else {
                        println!("{severity}: {}", diagnostic.message);
                    }
                }
            }
            Err(error) => {
                failed = true;
                eprintln!("=== {path}: {error}");
            }
        }
    }
    std::process::exit(if failed { 1 } else { 0 });
}
