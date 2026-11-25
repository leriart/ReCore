//! The options a theme's custom properties become: the kind each one is
//! offered as, what a `@property` rule makes of its value, and how long a
//! theme with many properties takes to read.

use std::time::{Duration, Instant};

use recore_css::{Mode, OptionKind, Options, ThemeOption, load_str};

/// The options of the theme read from `css`, with the dark palette.
fn options(css: &str) -> Vec<ThemeOption> {
    load_str(css, None, &Options::with_mode(Mode::Dark))
        .expect("readable")
        .options
}

/// The option the theme read from `css` offers for `name`.
fn option(css: &str, name: &str) -> ThemeOption {
    options(css)
        .into_iter()
        .find(|option| option.name == name)
        .unwrap_or_else(|| panic!("the theme offers {name}"))
}

/// A theme with `count` custom properties, each with a name of its own.
fn many_properties(count: usize) -> String {
    let mut css = String::from(":root {\n");
    for index in 0..count {
        use std::fmt::Write as _;
        let _ = writeln!(css, "--token-{index}: #{index:06x};");
    }
    css.push_str("}\n");
    css
}

#[test]
fn a_value_that_breaks_its_syntax_falls_back_to_the_initial_value() {
    let css = r#"
        @property --brand {
            syntax: "<color>";
            inherits: false;
            initial-value: #123456;
        }
        :root { --brand: not-a-colour; }
        "#;
    let brand = option(css, "--brand");
    assert!(matches!(brand.kind, OptionKind::Color { .. }));
    assert_eq!(brand.value, "#123456");
}

#[test]
fn a_syntax_names_the_type_of_a_value_the_text_does_not_reveal() {
    let css = r#"
        @property --gap {
            syntax: "<length>";
            inherits: false;
            initial-value: 4px;
        }
        :root { --gap: 12pt; }
        "#;
    assert!(matches!(
        option(css, "--gap").kind,
        OptionKind::Number { value: 12.0, unit } if unit == "pt"
    ));
}

#[test]
fn a_syntax_with_two_types_checks_each_of_them() {
    let css = r#"
        @property --brand {
            syntax: "<color> | <length>";
            inherits: false;
            initial-value: 4px;
        }
        :root { --brand: 12pt; }
        "#;
    assert!(matches!(
        option(css, "--brand").kind,
        OptionKind::Number { value: 12.0, unit } if unit == "pt"
    ));

    let css = r#"
        @property --brand {
            syntax: "<color> | <length>";
            inherits: false;
            initial-value: 4px;
        }
        :root { --brand: #abcdef; }
        "#;
    assert!(matches!(
        option(css, "--brand").kind,
        OptionKind::Color { .. }
    ));
}

#[test]
fn a_syntax_of_any_value_leaves_the_value_alone() {
    let css = r#"
        @property --anything {
            syntax: "*";
            inherits: false;
            initial-value: #123456;
        }
        :root { --anything: 12pt; }
        "#;
    let anything = option(css, "--anything");
    assert!(matches!(
        anything.kind,
        OptionKind::Number { value: 12.0, unit } if unit == "pt"
    ));
    assert_eq!(anything.value, "12pt");
}

#[test]
fn a_number_keeps_the_unit_the_theme_writes() {
    let css = ":root { --margin: 12pt; --width: 50vw; --wait: 300ms; }";
    assert!(matches!(
        option(css, "--margin").kind,
        OptionKind::Number { value: 12.0, unit } if unit == "pt"
    ));
    assert!(matches!(
        option(css, "--width").kind,
        OptionKind::Number { value: 50.0, unit } if unit == "vw"
    ));
    assert!(matches!(
        option(css, "--wait").kind,
        OptionKind::Number { value: 300.0, unit } if unit == "ms"
    ));
}

#[test]
fn an_opacity_is_a_fraction_and_another_name_is_a_percentage() {
    let css = ":root { --opacity: 0.5; --dim: 2; }";
    assert!(matches!(
        option(css, "--opacity").kind,
        OptionKind::Percent {
            value: 50.0,
            percent: false
        }
    ));
    assert!(matches!(
        option(css, "--dim").kind,
        OptionKind::Percent {
            value: 2.0,
            percent: true
        }
    ));
}

#[test]
fn a_picture_name_is_offered_as_a_picture() {
    let css = r#"
        :root {
            --wallpaper: url("bg.png");
            --banner: url("banner.png");
            --texture: url("grain.png");
        }
        "#;
    for name in ["--wallpaper", "--banner", "--texture"] {
        assert!(
            matches!(option(css, name).kind, OptionKind::Image { .. }),
            "{name} is offered as a picture"
        );
    }
}

#[test]
fn a_picture_name_with_a_colour_is_still_a_colour() {
    let css = ":root { --wallpaper-color: #ff0000; --header-color: #00ff00; }";
    assert!(matches!(
        option(css, "--wallpaper-color").kind,
        OptionKind::Color { .. }
    ));
    assert!(matches!(
        option(css, "--header-color").kind,
        OptionKind::Color { .. }
    ));
}

#[test]
fn a_comment_keeps_its_choices_when_the_value_is_not_among_them() {
    let css = r#"
        :root {
            --top-bar-position: floating; /* off: default, hide: no title, titlebar: move it up */
        }
        "#;
    let position = option(css, "--top-bar-position");
    assert!(matches!(
        &position.kind,
        OptionKind::Choice { choices }
            if choices.iter().any(|(_, value)| value == "floating")
                && choices.iter().any(|(_, value)| value == "titlebar")
    ));
}

#[test]
fn a_slider_keeps_its_range_when_the_value_is_outside_it() {
    let css = r#"
        /**
         * @name Slider
         * @var slider gap "Gap" 30 {min: 0, max: 20, step: 2}
         */
        :root { --gap: 30; }
        "#;
    assert!(matches!(
        option(css, "--gap").kind,
        OptionKind::Range {
            value: 30,
            min: 0,
            max: 30,
            step: 2,
            ..
        }
    ));
}

#[test]
fn a_flicker_reads_the_unit_of_its_duration() {
    let css = ":root { --rl-flicker-duration: 500ms; }";
    assert!(matches!(
        option(css, "--rl-flicker-duration").kind,
        OptionKind::Toggle {
            on: true,
            on_value,
            off_value,
        } if on_value == "6000ms" && off_value == "0ms"
    ));

    let css = ":root { --rl-flicker-duration: 0s; }";
    assert!(matches!(
        option(css, "--rl-flicker-duration").kind,
        OptionKind::Toggle { on: false, .. }
    ));
}

#[test]
fn a_long_picture_is_offered_under_any_name() {
    let data = format!("url(\"data:image/png;base64,{}\")", "A".repeat(3000));
    let css = format!(":root {{ --wallpaper: {data}; }}");
    let wallpaper = option(&css, "--wallpaper");
    assert!(matches!(
        wallpaper.kind,
        OptionKind::Image {
            embedded: true,
            url: None
        }
    ));
}

#[test]
fn a_theme_with_many_properties_is_read_in_a_bounded_time() {
    let css = many_properties(20_000);
    let started = Instant::now();
    let theme = load_str(&css, None, &Options::with_mode(Mode::Dark)).expect("readable");
    let took = started.elapsed();
    assert_eq!(theme.options.len(), 20_000);
    // A merge that compares every name with every other name needs minutes
    // here; reading a theme with this many properties must stay in seconds.
    let budget = if cfg!(debug_assertions) {
        Duration::from_secs(60)
    } else {
        Duration::from_secs(20)
    };
    assert!(took < budget, "reading 20000 properties took {took:?}");
}
