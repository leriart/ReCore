//! What a theme may name and the reader will not follow: files that are not
//! regular files, files larger than the reader keeps, and an import that
//! leads back to itself. Each case ends with the rest of the theme read and
//! a warning that says what was left out, never a hang, an endless read or
//! a window that never opens.

use std::fs;
use std::path::{Path, PathBuf};

use recore_css::{Rgba, load};

/// A folder of this test's own, emptied first.
fn folder(name: &str) -> PathBuf {
    let folder =
        std::env::temp_dir().join(format!("recore-css-limits-{}-{name}", std::process::id()));
    let _ = fs::remove_dir_all(&folder);
    fs::create_dir_all(&folder).expect("the folder");
    folder
}

fn says(theme: &recore_css::ThemeFile, words: &str) -> bool {
    theme.warnings.iter().any(|warning| warning.contains(words))
}

#[test]
fn a_local_import_larger_than_the_reader_keeps_is_left_out() {
    let folder = folder("large");
    fs::write(
        folder.join("main.theme.css"),
        "@import \"huge.css\";\n:root { --background-primary: #102030; }\n",
    )
    .expect("the theme");
    // A hole of 9 MiB and one byte shows the reader stops at the limit
    // without writing nine megabytes to disk.
    let huge = fs::File::create(folder.join("huge.css")).expect("the sheet");
    huge.set_len(9 * 1024 * 1024).expect("its size");
    drop(huge);

    let theme = load(folder.join("main.theme.css")).expect("the theme is read");
    assert_eq!(theme.palette.chat, Rgba::rgb(0x10, 0x20, 0x30));
    assert!(
        says(&theme, "larger than 8 MiB"),
        "a warning says what was left out: {:?}",
        theme.warnings
    );
    fs::remove_dir_all(&folder).expect("cleanup");
}

#[test]
fn a_local_import_that_is_not_a_file_is_left_out() {
    let folder = folder("directory");
    fs::write(
        folder.join("main.theme.css"),
        "@import \"sub\";\n:root { --background-primary: #102030; }\n",
    )
    .expect("the theme");
    fs::create_dir_all(folder.join("sub")).expect("the folder named as a sheet");

    let theme = load(folder.join("main.theme.css")).expect("the theme is read");
    assert_eq!(theme.palette.chat, Rgba::rgb(0x10, 0x20, 0x30));
    assert!(
        says(&theme, "not a regular file"),
        "a directory is no stylesheet: {:?}",
        theme.warnings
    );
    fs::remove_dir_all(&folder).expect("cleanup");
}

#[cfg(unix)]
#[test]
fn a_local_import_that_is_a_pipe_is_left_out_without_waiting() {
    let folder = folder("fifo");
    fs::write(
        folder.join("main.theme.css"),
        "@import \"pipe.css\";\n:root { --background-primary: #102030; }\n",
    )
    .expect("the theme");
    let pipe = folder.join("pipe.css");
    // `mkfifo` through the system's own tool: the reader must not open it.
    let made = std::process::Command::new("mkfifo")
        .arg(&pipe)
        .status()
        .expect("mkfifo");
    assert!(made.success(), "a FIFO beside the theme");

    // A reader that opened the pipe would wait for its writer here; the
    // metadata check turns that into a warning.
    let started = std::time::Instant::now();
    let theme = load(folder.join("main.theme.css")).expect("the theme is read");
    assert!(
        started.elapsed() < std::time::Duration::from_secs(5),
        "the pipe did not hold the reader"
    );
    assert_eq!(theme.palette.chat, Rgba::rgb(0x10, 0x20, 0x30));
    assert!(
        says(&theme, "not a regular file"),
        "a pipe is no stylesheet: {:?}",
        theme.warnings
    );
    fs::remove_dir_all(&folder).expect("cleanup");
}

#[test]
fn an_import_that_leads_back_to_itself_is_read_once() {
    let folder = folder("cycle");
    fs::write(
        folder.join("main.theme.css"),
        "@import \"one.css\";\n:root { --background-primary: #102030; }\n",
    )
    .expect("the theme");
    fs::write(
        folder.join("one.css"),
        "@import \"two.css\";\n:root { --text-normal: #eeeeee; }\n",
    )
    .expect("the first sheet");
    fs::write(
        folder.join("two.css"),
        "@import \"one.css\";\n:root { --brand-experiment: #123456; }\n",
    )
    .expect("the second sheet");

    let started = std::time::Instant::now();
    let theme = load(folder.join("main.theme.css")).expect("the theme is read");
    assert!(
        started.elapsed() < std::time::Duration::from_secs(5),
        "a cycle is caught, not read sixteen times"
    );
    // The sheet that imported itself is skipped, and what the cycle brought
    // in stays.
    assert_eq!(theme.palette.accent, Rgba::rgb(0x12, 0x34, 0x56));
    assert_eq!(theme.palette.text, Rgba::rgb(0xee, 0xee, 0xee));
    assert!(
        says(&theme, "already read") || says(&theme, "imports itself"),
        "a warning names the loop: {:?}",
        theme.warnings
    );
    fs::remove_dir_all(&folder).expect("cleanup");
}

#[test]
fn a_theme_that_is_itself_a_directory_is_not_a_theme() {
    let folder = folder("not-a-theme");
    fs::create_dir_all(folder.join("theme.css")).expect("the directory");
    let refused = load(folder.join("theme.css"));
    assert!(refused.is_err(), "a directory is no theme");
}

#[test]
fn a_file_that_is_not_utf8_still_says_which_file_it_was() {
    let folder = folder("not-utf8");
    let path: &Path = &folder.join("bad.theme.css");
    fs::write(
        path,
        b":root { --background-primary: #102030; } /* \xff\xfe */\n",
    )
    .expect("the file");
    let error = load(path).expect_err("not a theme");
    assert!(error.contains("bad.theme.css"), "{error}");
    fs::remove_dir_all(&folder).expect("cleanup");
}
