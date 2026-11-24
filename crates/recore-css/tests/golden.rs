//! A golden snapshot of the interpreter over the themes ReCore ships.
//!
//! Each bundled `.css` theme is read through [`recore_css::load`] and reduced
//! to one stable line: identity, the panel colours, the accent, the radius,
//! the font family and the counts of options and fonts. The lines, one
//! `=== <file>` section per theme, are compared with the versioned snapshot
//! in `tests/fixtures/themes.snap`, so a change to the interpreter or to a
//! bundled theme shows as a readable diff.
//!
//! Rewrite the snapshot with `UPDATE_GOLDEN=1 cargo test -p recore-css --test
//! golden`, and read the diff the snapshot gives before committing it.

use std::{
    fmt::Write as _,
    fs,
    path::{Path, PathBuf},
};

/// The snapshot, relative to this crate's folder.
const SNAPSHOT: &str = "tests/fixtures/themes.snap";

/// The variable that makes the test write the snapshot instead of comparing.
const UPDATE_VARIABLE: &str = "UPDATE_GOLDEN";

fn themes_folder() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../themes")
}

fn snapshot_path() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join(SNAPSHOT)
}

/// The bundled themes, by file name.
fn bundled_themes() -> Vec<PathBuf> {
    let folder = themes_folder();
    let mut themes: Vec<PathBuf> = fs::read_dir(&folder)
        .unwrap_or_else(|error| panic!("could not read {}: {error}", folder.display()))
        .map(|entry| entry.expect("an entry").path())
        .filter(|path| path.extension().is_some_and(|kind| kind == "css"))
        .collect();
    themes.sort_by(|left, right| left.file_name().cmp(&right.file_name()));
    assert!(!themes.is_empty(), "no .css themes in {}", folder.display());
    themes
}

/// One `=== <file>` section per theme, with a fixed field order.
fn render() -> String {
    let mut golden = String::new();
    for path in bundled_themes() {
        let file = path
            .file_name()
            .expect("a theme file has a name")
            .to_string_lossy();
        let theme = recore_css::load(&path)
            .unwrap_or_else(|error| panic!("could not read {file}: {error}"));
        let palette = &theme.palette;
        writeln!(golden, "=== {file}").expect("a string");
        writeln!(
            golden,
            "{} | {} | {} | {} | {} | {} | {} | {} | {:?} | {:?} | {} | {}",
            theme.id,
            theme.name,
            palette.dark,
            palette.chat.hex(),
            palette.sidebar.hex(),
            palette.rail.hex(),
            palette.text.hex(),
            palette.accent.hex(),
            theme.look.radius,
            theme.look.font_family,
            theme.options.len(),
            theme.fonts.len(),
        )
        .expect("a string");
    }
    golden
}

/// The first line that differs, and how the two sides line up.
fn difference(expected: &str, actual: &str) -> String {
    let mut message = String::new();
    let mut lined_up = true;
    for (index, (want, got)) in expected.lines().zip(actual.lines()).enumerate() {
        if want != got {
            writeln!(
                message,
                "first difference at line {}:\n  snapshot: {want}\n  themes:   {got}",
                index + 1
            )
            .expect("a string");
            lined_up = false;
            break;
        }
    }
    if lined_up {
        writeln!(
            message,
            "the snapshot has {} lines and the themes give {}",
            expected.lines().count(),
            actual.lines().count()
        )
        .expect("a string");
    }
    message
}

#[test]
fn bundled_themes_match_golden_snapshot() {
    let golden = render();
    let path = snapshot_path();

    if std::env::var_os(UPDATE_VARIABLE).is_some() {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)
                .unwrap_or_else(|error| panic!("could not create {}: {error}", parent.display()));
        }
        fs::write(&path, &golden)
            .unwrap_or_else(|error| panic!("could not write {}: {error}", path.display()));
        return;
    }

    let expected = fs::read_to_string(&path).unwrap_or_else(|error| {
        panic!(
            "could not read {}: {error}\nrun with UPDATE_GOLDEN=1 to rewrite the snapshot",
            path.display()
        )
    });
    if expected == golden {
        return;
    }

    panic!(
        "{} does not match the bundled themes\n{}\nrun with UPDATE_GOLDEN=1 to rewrite the snapshot",
        path.display(),
        difference(&expected, &golden),
    );
}
