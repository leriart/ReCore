//! A versioned conformance suite for the theme reader.
//!
//! Every `.css` file in `tests/fixtures/conformance/` carries, on its first
//! line, the observation CSS asks for through ReCore's documented region
//! mapping:
//!
//! ```css
//! /* expect: native.sidebar=#ff0000 native.category_size=12 warnings=1 */
//! ```
//!
//! The test loads each fixture in dark mode with [`recore_css::load_with`],
//! observes the named keys and compares them with the comment. `warnings`
//! counts `ThemeFile::warnings`; a key with an empty value means the reader
//! must have no value for it (`None` or an empty list). Keys a fixture does
//! not name are not checked. An observation is one token: a colour is its
//! hex, a number its decimal form, a shadow `x,y,blur,#rrggbb`, a font
//! `family:weight:italic:bytes`, an animation
//! `name:seconds:iterations:direction:frames` and a list joins with `|`.
//!
//! `tests/fixtures/conformance.snap` records one line per fixture, in
//! alphabetical order, as `<file>: <key>=<value> ...`. It is the history of
//! what the suite reads: rewrite it with
//! `UPDATE_CONFORMANCE=1 cargo test -p recore-css --test conformance`, and
//! read the diff before committing it. The comment in the fixture, not the
//! snapshot, is what CSS asks for; updating the snapshot never silences a
//! conformance failure.

use std::{
    fmt::Write as _,
    fs,
    path::{Path, PathBuf},
};

use recore_css::{
    Animation, Font, Image, Look, Mode, NativeStyle, Options, Palette, Rgba, Shadow, TextTransform,
    ThemeFile, load_with,
};

/// The fixtures, relative to this crate's folder.
const FIXTURES: &str = "tests/fixtures/conformance";

/// The snapshot, relative to this crate's folder.
const SNAPSHOT: &str = "tests/fixtures/conformance.snap";

/// The variable that makes the test write the snapshot instead of comparing.
const UPDATE_VARIABLE: &str = "UPDATE_CONFORMANCE";

/// The floor the suite keeps, so a fixture cannot quietly disappear.
const MINIMUM_FIXTURES: usize = 18;

/// One fixture, its file name and the expectations its first line names.
struct Fixture {
    path: PathBuf,
    file: String,
    expected: Vec<(String, String)>,
}

fn fixtures_folder() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join(FIXTURES)
}

fn snapshot_path() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join(SNAPSHOT)
}

/// The fixtures, by file name, as [`Fixture`]s.
fn fixtures() -> Vec<Fixture> {
    let folder = fixtures_folder();
    let mut paths: Vec<PathBuf> = fs::read_dir(&folder)
        .unwrap_or_else(|error| panic!("could not read {}: {error}", folder.display()))
        .map(|entry| entry.expect("an entry").path())
        .filter(|path| path.extension().is_some_and(|kind| kind == "css"))
        .collect();
    paths.sort_by(|left, right| left.file_name().cmp(&right.file_name()));
    assert!(
        paths.len() >= MINIMUM_FIXTURES,
        "the conformance suite has {} fixtures in {}, fewer than {MINIMUM_FIXTURES}",
        paths.len(),
        folder.display()
    );
    paths.into_iter().map(read_fixture).collect()
}

/// Reads one fixture and the expectations on its first line.
fn read_fixture(path: PathBuf) -> Fixture {
    let file = path
        .file_name()
        .expect("a fixture has a name")
        .to_string_lossy()
        .into_owned();
    let css =
        fs::read_to_string(&path).unwrap_or_else(|error| panic!("could not read {file}: {error}"));
    let Some(first) = css.lines().next() else {
        panic!("{file}: the first line must be `/* expect: ... */`");
    };
    let body = first
        .strip_prefix("/* expect:")
        .and_then(|rest| rest.trim_end().strip_suffix("*/"))
        .unwrap_or_else(|| {
            panic!("{file}: the first line must be `/* expect: ... */`, not `{first}`")
        });

    let mut expected: Vec<(String, String)> = Vec::new();
    for token in body.split_whitespace() {
        let (key, value) = token.split_once('=').unwrap_or_else(|| {
            panic!("{file}: the expectation `{token}` has no `=`; values cannot contain spaces")
        });
        assert!(!key.is_empty(), "{file}: an expectation has no key");
        assert!(
            !expected.iter().any(|(seen, _)| seen == key),
            "{file}: the key `{key}` is named twice"
        );
        expected.push((key.to_owned(), value.to_owned()));
    }
    assert!(!expected.is_empty(), "{file}: the expectation is empty");
    Fixture {
        path,
        file,
        expected,
    }
}

/// What the reader made of each named key, in the fixture's order.
fn observe_fixture(fixture: &Fixture) -> Vec<(String, String)> {
    let theme = load_with(&fixture.path, &Options::with_mode(Mode::Dark))
        .unwrap_or_else(|error| panic!("could not read {}: {error}", fixture.file));
    fixture
        .expected
        .iter()
        .map(|(key, _)| {
            let value = observe(&theme, key)
                .unwrap_or_else(|problem| panic!("{}: {problem}", fixture.file));
            (key.clone(), value)
        })
        .collect()
}

/// The observation for one key. A key the suite does not know is an error,
/// not an empty value, so a typo cannot pass unnoticed.
fn observe(theme: &ThemeFile, key: &str) -> Result<String, String> {
    if let Some(field) = key.strip_prefix("palette.") {
        return palette_value(&theme.palette, field);
    }
    if let Some(field) = key.strip_prefix("native.") {
        return native_value(&theme.native, field);
    }
    if let Some(field) = key.strip_prefix("look.") {
        return look_value(&theme.look, field);
    }
    match key {
        "warnings" => Ok(theme.warnings.len().to_string()),
        "fonts" => Ok(fonts(&theme.fonts)),
        "fonts.count" => Ok(theme.fonts.len().to_string()),
        "options" => Ok(theme.options.len().to_string()),
        "page_layout" => Ok(theme.page_layout.map_or_else(String::new, |layout| {
            format!(
                "{},{},{},{}",
                layout.rail, layout.sidebar, layout.chat, layout.members
            )
        })),
        "remote" => Ok(theme.remote.to_string()),
        _ => Err(format!("`{key}` is not a key this suite observes")),
    }
}

fn palette_value(palette: &Palette, field: &str) -> Result<String, String> {
    Ok(match field {
        "dark" => palette.dark.to_string(),
        "rail" => palette.rail.hex(),
        "sidebar" => palette.sidebar.hex(),
        "panel" => palette.panel.hex(),
        "chat" => palette.chat.hex(),
        "stage" => palette.stage.hex(),
        "raised" => palette.raised.hex(),
        "field" => palette.field.hex(),
        "sunken" => palette.sunken.hex(),
        "hover" => palette.hover.hex(),
        "active" => palette.active.hex(),
        "row_hover" => palette.row_hover.hex(),
        "line" => palette.line.hex(),
        "text" => palette.text.hex(),
        "secondary" => palette.secondary.hex(),
        "muted" => palette.muted.hex(),
        "faint" => palette.faint.hex(),
        "accent" => palette.accent.hex(),
        "danger" => palette.danger.hex(),
        "live" => palette.live.hex(),
        "link" => palette.link.hex(),
        "tooltip" => palette.tooltip.hex(),
        _ => {
            return Err(format!(
                "`palette.{field}` is not a key this suite observes"
            ));
        }
    })
}

fn native_value(native: &NativeStyle, field: &str) -> Result<String, String> {
    let colour = |colour: Option<Rgba>| colour.map_or_else(String::new, |colour| colour.hex());
    let number = |value: Option<f32>| value.map_or_else(String::new, |value| value.to_string());
    Ok(match field {
        "rail" => colour(native.rail),
        "sidebar" => colour(native.sidebar),
        "chat" => colour(native.chat),
        "panel" => colour(native.panel),
        "raised" => colour(native.raised),
        "field" => colour(native.field),
        "stage" => colour(native.stage),
        "tooltip" => colour(native.tooltip),
        "text" => colour(native.text),
        "secondary" => colour(native.secondary),
        "muted" => colour(native.muted),
        "hover" => colour(native.hover),
        "row_hover" => colour(native.row_hover),
        "line" => colour(native.line),
        "accent" => colour(native.accent),
        "selection" => colour(native.selection),
        "placeholder" => colour(native.placeholder),
        "scrollbar" => colour(native.scrollbar),
        "scrollbar_hover" => colour(native.scrollbar_hover),
        "raised_shadow" => shadow(&native.raised_shadow),
        "card_shadow" => shadow(&native.card_shadow),
        "rail_opacity" => number(native.rail_opacity),
        "sidebar_opacity" => number(native.sidebar_opacity),
        "chat_opacity" => number(native.chat_opacity),
        "panel_opacity" => number(native.panel_opacity),
        "raised_opacity" => number(native.raised_opacity),
        "field_opacity" => number(native.field_opacity),
        "stage_opacity" => number(native.stage_opacity),
        "tooltip_opacity" => number(native.tooltip_opacity),
        "radius" => number(native.radius),
        "font_family" => native.font_family.clone().unwrap_or_default(),
        "font_size" => number(native.font_size),
        "letter_spacing" => number(native.letter_spacing),
        "category_size" => number(native.category_size),
        "category_transform" => transform(&native.category_transform),
        "member_heading_size" => number(native.member_heading_size),
        "member_heading_transform" => transform(&native.member_heading_transform),
        "header_size" => number(native.header_size),
        "header_transform" => transform(&native.header_transform),
        "motion_ms" => native
            .motion_ms
            .map_or_else(String::new, |ms| ms.to_string()),
        "motion_steps" => native
            .motion_steps
            .map_or_else(String::new, |steps| steps.to_string()),
        _ => return Err(format!("`native.{field}` is not a key this suite observes")),
    })
}

fn look_value(look: &Look, field: &str) -> Result<String, String> {
    let number = |value: Option<f32>| value.map_or_else(String::new, |value| value.to_string());
    Ok(match field {
        "font_family" => look.font_family.clone().unwrap_or_default(),
        "font_size" => number(look.font_size),
        "letter_spacing" => number(look.letter_spacing),
        "radius" => number(look.radius),
        "frame" => look.frame.map_or_else(String::new, |frame| frame.hex()),
        "panel_gap" => number(look.panel_gap),
        "panel_radius" => number(look.panel_radius),
        "motion" => format!("{:?}", look.motion),
        "pixel_icons" => look.pixel_icons.to_string(),
        "background" => image(look.background.as_ref()),
        "background_dim" => number(look.background_dim),
        "panel_opacity" => number(look.panel_opacity),
        "background_fit" => format!("{:?}", look.background_fit),
        "accent" => look.accent.map_or_else(String::new, |accent| accent.hex()),
        "animations" => animations(&look.animations),
        _ => return Err(format!("`look.{field}` is not a key this suite observes")),
    })
}

fn shadow(shadow: &Option<Shadow>) -> String {
    shadow.as_ref().map_or_else(String::new, |shadow| {
        format!(
            "{},{},{},{}",
            shadow.offset_x,
            shadow.offset_y,
            shadow.blur,
            shadow.color.hex()
        )
    })
}

fn transform(transform: &Option<TextTransform>) -> String {
    transform.map_or_else(String::new, |transform| format!("{transform:?}"))
}

fn image(image: Option<&Image>) -> String {
    match image {
        None => String::new(),
        Some(Image::Data(bytes)) => format!("data[{}]", bytes.len()),
        Some(Image::File(path)) => {
            let name = path
                .file_name()
                .map_or_else(String::new, |name| name.to_string_lossy().into_owned());
            format!("file[{name}]")
        }
        Some(Image::Url(url)) => format!("url[{url}]"),
        Some(Image::Gradient(gradient)) => {
            let stops = gradient
                .stops
                .iter()
                .map(|stop| stop.color.hex())
                .collect::<Vec<_>>()
                .join(",");
            format!("{:?}[{stops}]", gradient.kind)
        }
    }
}

fn fonts(fonts: &[Font]) -> String {
    fonts
        .iter()
        .map(|font| {
            format!(
                "{}:{}:{}:{}b",
                font.family,
                font.weight,
                font.italic,
                font.bytes.len()
            )
        })
        .collect::<Vec<_>>()
        .join("|")
}

fn animations(animations: &[Animation]) -> String {
    animations
        .iter()
        .map(|animation| {
            let iterations = animation
                .iterations
                .map_or_else(|| "inf".to_owned(), |iterations| iterations.to_string());
            format!(
                "{}:{}s:{}:{:?}:{}f",
                animation.name,
                animation.duration,
                iterations,
                animation.direction,
                animation.frames.len()
            )
        })
        .collect::<Vec<_>>()
        .join("|")
}

/// One `file: key=value ...` line per fixture, in alphabetical order.
fn render() -> String {
    let mut snapshot = String::new();
    for fixture in fixtures() {
        let observed = observe_fixture(&fixture);
        let values = observed
            .iter()
            .map(|(key, value)| format!("{key}={value}"))
            .collect::<Vec<_>>()
            .join(" ");
        writeln!(snapshot, "{}: {values}", fixture.file).expect("a string");
    }
    snapshot
}

/// The first line that differs, and how the two sides line up.
fn difference(expected: &str, actual: &str) -> String {
    let mut message = String::new();
    let mut lined_up = true;
    for (index, (want, got)) in expected.lines().zip(actual.lines()).enumerate() {
        if want != got {
            writeln!(
                message,
                "first difference at line {}:\n  snapshot: {want}\n  fixtures: {got}",
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
            "the snapshot has {} lines and the fixtures give {}",
            expected.lines().count(),
            actual.lines().count()
        )
        .expect("a string");
    }
    message
}

#[test]
fn fixtures_match_their_expectations() {
    let mut problems = Vec::new();
    for fixture in fixtures() {
        let observed = observe_fixture(&fixture);
        for ((key, want), (_, got)) in fixture.expected.iter().zip(&observed) {
            if want == got {
                continue;
            }
            let want = if want.is_empty() {
                "(no value)".to_owned()
            } else {
                want.clone()
            };
            let got = if got.is_empty() {
                "(no value)".to_owned()
            } else {
                got.clone()
            };
            problems.push(format!(
                "{}: {key}: CSS asks for `{want}`, read `{got}`",
                fixture.file
            ));
        }
    }
    assert!(
        problems.is_empty(),
        "the fixtures do not match what CSS asks for:\n{}",
        problems.join("\n")
    );
}

#[test]
fn fixtures_match_the_snapshot() {
    let rendered = render();
    let path = snapshot_path();

    if std::env::var_os(UPDATE_VARIABLE).is_some() {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)
                .unwrap_or_else(|error| panic!("could not create {}: {error}", parent.display()));
        }
        fs::write(&path, &rendered)
            .unwrap_or_else(|error| panic!("could not write {}: {error}", path.display()));
        return;
    }

    let expected = fs::read_to_string(&path).unwrap_or_else(|error| {
        panic!(
            "could not read {}: {error}\nrun with UPDATE_CONFORMANCE=1 to write it",
            path.display()
        )
    });
    if expected == rendered {
        return;
    }

    panic!(
        "{} does not match the fixtures\n{}\nrun with UPDATE_CONFORMANCE=1 to rewrite it",
        path.display(),
        difference(&expected, &rendered),
    );
}
