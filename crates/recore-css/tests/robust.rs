//! No CSS, however broken, may bring the window down: the interpreter
//! reads every input here to the end without a panic, a stack overflow or
//! a run that never ends.

use std::time::{Duration, Instant};

use recore_css::{Mode, Options, load_str};

/// Reads `css` on a small stack, as a deep theme would meet on a worker
/// thread, and says whether it came back in time without a panic.
fn survives(name: &str, css: String) {
    let label = name.to_owned();
    let started = Instant::now();
    let (done, waited) = std::sync::mpsc::channel();
    std::thread::Builder::new()
        .stack_size(1 << 20)
        .spawn(move || {
            for mode in [Mode::Auto, Mode::Dark, Mode::Light] {
                let _ = load_str(&css, None, &Options::with_mode(mode));
            }
            let _ = done.send(());
        })
        .expect("a thread");
    // A debug build reads a damaged 447 KiB theme several times slower than
    // an optimized one; the check is that the reader ends at all, not how
    // fast it is, so it gets a budget that fits the build.
    let (wait, total) = if cfg!(debug_assertions) {
        (Duration::from_secs(90), Duration::from_secs(180))
    } else {
        (Duration::from_secs(10), Duration::from_secs(20))
    };
    assert!(
        waited.recv_timeout(wait).is_ok(),
        "{label}: still running after {wait:?}"
    );
    assert!(
        started.elapsed() < total,
        "{label}: took {:?}",
        started.elapsed()
    );
}

/// A small generator, so each run tries the same inputs.
struct Dice(u64);

impl Dice {
    fn roll(&mut self, below: usize) -> usize {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        (self.0 % below.max(1) as u64) as usize
    }
}

fn bundled() -> Vec<(String, String)> {
    let folder = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../themes");
    let mut themes = Vec::new();
    for entry in std::fs::read_dir(folder).expect("the bundled themes") {
        let path = entry.expect("an entry").path();
        if path.extension().is_some_and(|kind| kind == "css") {
            themes.push((
                path.file_name()
                    .expect("a name")
                    .to_string_lossy()
                    .into_owned(),
                std::fs::read_to_string(&path).expect("a theme"),
            ));
        }
    }
    assert!(!themes.is_empty());
    themes
}

#[test]
fn hostile_inputs_are_read_without_a_panic() {
    let deep = |open: &str, close: &str, depth: usize| {
        format!(
            ":root {{ --a: {}1{}; color: var(--a); }}",
            open.repeat(depth),
            close.repeat(depth)
        )
    };
    let cases: Vec<(&str, String)> = vec![
        ("empty", String::new()),
        ("only whitespace", " \n\t\r\n".into()),
        ("lone brace", "{".into()),
        ("lone close", "}".into()),
        ("many closes", "}".repeat(5000)),
        ("many opens", "a{".repeat(20_000)),
        ("nested at-rules", "@media x{".repeat(20_000)),
        ("unterminated comment", ":root{--a:red;} /* never ends".into()),
        ("unterminated string", ":root{--a:\"never ends; --b: blue}".into()),
        ("unterminated url", ":root{background:url(http://x".into()),
        ("unterminated block", ":root{--a:red".into()),
        ("escape at end", ":root{--a:\\".into()),
        ("nul bytes", ":root{--a:\0\0;}\0".into()),
        ("bom", "\u{feff}:root{--background-primary:#111}".into()),
        ("self reference", ":root{--a:var(--a); --background-primary:var(--a)}".into()),
        (
            "mutual reference",
            ":root{--a:var(--b);--b:var(--c);--c:var(--a);--background-primary:var(--b)}".into(),
        ),
        (
            "fallback chain",
            format!(
                ":root{{--background-primary:{}#123{}}}",
                "var(--x,".repeat(3000),
                ")".repeat(3000)
            ),
        ),
        (
            "exponential variables",
            {
                use std::fmt::Write as _;
                let mut css = String::from(":root{--v0:#111 #111;");
                for step in 1..64 {
                    let _ = write!(
                        css,
                        "--v{step}:var(--v{}) var(--v{});",
                        step - 1,
                        step - 1
                    );
                }
                css + "--background-primary:var(--v63)}"
            },
        ),
        ("deep calc", deep("calc(", ")", 5000)),
        ("deep parens", deep("(", ")", 50_000)),
        ("deep color-mix", {
            format!(
                ":root{{--background-primary:{}red{}}}",
                "color-mix(in srgb,".repeat(2000),
                ",blue)".repeat(2000)
            )
        }),
        ("deep gradient", {
            format!(
                "body{{background:{}red{}}}",
                "linear-gradient(".repeat(3000),
                ")".repeat(3000)
            )
        }),
        ("huge numbers", ":root{--radius:1e999px;--a:rgb(1e999 -1e999 NaN / inf);--background-primary:var(--a);font-size:99999999999999999999999px}".into()),
        ("division by zero", ":root{--a:calc(1px / 0);--b:calc(0 / 0);border-radius:var(--a);opacity:var(--b)}".into()),
        ("bad hex", ":root{--background-primary:#;--b:#12;--c:#12345;--d:#1234567;--e:#zzzzzz;--f:#ééé}".into()),
        ("unicode in colours", ":root{--background-primary:#é12345;--b:rgb(１２,３,４);--c:hsl(é,é,é)}".into()),
        ("multi-byte everywhere", "é{é:é}@é é{é{é:é;}}/*é*/".repeat(200)),
        ("bad percentages", ":root{--background-primary:hsl(400deg -50% 900%);--b:rgba(300,300,300,9)}".into()),
        ("empty functions", ":root{--a:rgb();--b:hsl();--c:var();--d:calc();--e:url();--f:color-mix();--background-primary:var(--a);background:linear-gradient()}".into()),
        ("keyframes without body", "@keyframes x; @keyframes {} @keyframes y { from } body{animation:y 0s infinite}".into()),
        ("zero duration animation", "@keyframes y{0%{background:red}100%{background:blue}} body{animation:y 0s infinite, y -1s, y 1e999s}".into()),
        ("odd keyframe stops", "@keyframes y{-10%{opacity:0}1e9%{opacity:1}NaN%{opacity:2}} body{animation:y 1s steps(0) infinite}".into()),
        ("import of itself", "@import url(\"data:text/css,@import url(x)\"); @import ''; @import;".into()),
        ("data url", ":root{background-image:url(data:image/png;base64,!!!!)}body{background:url(data:;base64,)}".into()),
        ("font face junk", "@font-face{font-family:;src:url()}@font-face{src:url(data:font/ttf;base64,AAAA) format(}".into()),
        ("long selector", format!("{}{{color:red}}", ".a ".repeat(100_000))),
        ("many declarations", format!(":root{{{}}}", "--a:red;".repeat(200_000))),
        ("long value", format!(":root{{--a:{};}}", "a".repeat(2_000_000))),
        ("meta junk", "/**\n * @name\n * @var\n * @var color\n * @var range x \"\" [0,0,0,0]\n * @var select y \"\" {\n */".into()),
        ("meta without end", "/**\n * @name x\n * @var color a \"A\" #fff".into()),
    ];
    for (name, css) in cases {
        survives(name, css);
    }
}

#[test]
fn a_bundled_theme_cut_anywhere_is_read_without_a_panic() {
    // A debug build reads a 447 KiB theme about ten times slower than an
    // optimized one, so it samples fewer cuts: the check is that no cut
    // panics, and the optimized runs (the `fast` profile, `--release`) keep
    // the full sweep.
    let cuts = if cfg!(debug_assertions) { 24 } else { 120 };
    for (name, css) in bundled() {
        let step = (css.len() / cuts).max(1);
        let mut at = 0;
        while at < css.len() {
            if css.is_char_boundary(at) {
                survives(&format!("{name} cut at {at}"), css[..at].to_owned());
            }
            at += step;
        }
    }
}

#[test]
fn a_bundled_theme_with_random_damage_is_read_without_a_panic() {
    const JUNK: &[&str] = &[
        "{",
        "}",
        "(",
        ")",
        ";",
        ":",
        "\"",
        "'",
        "/*",
        "*/",
        "\\",
        "@",
        "#",
        "%",
        ",",
        "var(",
        "calc(",
        "url(",
        "-",
        "0",
        "é",
        "\n",
        "!important",
        "--",
        "1e999",
        "rgb(",
        "@import ",
        "@keyframes ",
        "@media ",
        "::",
        "[",
        "]",
    ];
    let mut dice = Dice(0x9e37_79b9_7f4a_7c15);
    let rounds = if cfg!(debug_assertions) { 6 } else { 24 };
    for (name, css) in bundled() {
        for round in 0..rounds {
            let mut damaged = css.clone();
            for _ in 0..1 + dice.roll(12) {
                let mut at = dice.roll(damaged.len());
                while !damaged.is_char_boundary(at) {
                    at -= 1;
                }
                match dice.roll(3) {
                    0 => damaged.insert_str(at, JUNK[dice.roll(JUNK.len())]),
                    1 => {
                        let mut end = (at + 1 + dice.roll(40)).min(damaged.len());
                        while !damaged.is_char_boundary(end) {
                            end += 1;
                        }
                        damaged.replace_range(at..end, "");
                    }
                    _ => {
                        let mut end = (at + dice.roll(200)).min(damaged.len());
                        while !damaged.is_char_boundary(end) {
                            end += 1;
                        }
                        let piece = damaged[at..end].to_owned();
                        let mut to = dice.roll(damaged.len());
                        while !damaged.is_char_boundary(to) {
                            to -= 1;
                        }
                        damaged.insert_str(to, &piece);
                    }
                }
            }
            survives(&format!("{name} damaged, round {round}"), damaged);
        }
    }
}
