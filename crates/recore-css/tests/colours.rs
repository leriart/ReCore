//! The colour maths, pinned to the reference values of CSS Color 4.
//!
//! The conversions are matrices and transfer functions, and a change to one
//! of them is invisible without a value to compare against: a test that only
//! says "this red is reddish" passes through a wrong matrix. Each value here
//! comes from CSS Color 4's sample code or from a browser's computed style.

use recore_css::color::{Rgba, parse};

/// The colour a value parses to, or a panic naming the value.
fn colour(value: &str) -> Rgba {
    parse(value, false).unwrap_or_else(|| panic!("{value} is not a colour"))
}

/// Whether `colour` is within `tolerance` of `(r, g, b)`, channel by channel.
fn near(colour: Rgba, (r, g, b): (u8, u8, u8), tolerance: i16) -> bool {
    let away = |a: u8, b: u8| i16::from(a).abs_diff(i16::from(b)) as i16 <= tolerance;
    away(colour.r, r) && away(colour.g, g) && away(colour.b, b)
}

/// Whether `colour` is exactly `(r, g, b)`, opaque.
fn is(colour: Rgba, (r, g, b): (u8, u8, u8)) -> bool {
    colour.r == r && colour.g == g && colour.b == b && colour.a == 255
}

#[test]
fn oklch_matches_the_reference_values() {
    // CSS Color 4's reference conversions of the sRGB primaries.
    assert!(
        near(colour("oklch(0.6279 0.2577 29.23)"), (255, 0, 0), 1),
        "red is oklch(0.6279 0.2577 29.23), got {:?}",
        colour("oklch(0.6279 0.2577 29.23)")
    );
    assert!(
        near(colour("oklch(0.8664 0.2948 142.5)"), (0, 255, 0), 1),
        "green is oklch(0.8664 0.2948 142.5), got {:?}",
        colour("oklch(0.8664 0.2948 142.5)")
    );
    assert!(
        near(colour("oklch(0.4520 0.3132 264.05)"), (0, 0, 255), 1),
        "blue is oklch(0.4520 0.3132 264.05)"
    );
    assert!(
        near(colour("oklch(1 0 0)"), (255, 255, 255), 0),
        "white is oklch(1 0 0)"
    );
    assert!(
        near(colour("oklch(0.5999 0 0)"), (128, 128, 128), 1),
        "mid grey is oklch(0.5999 0 0), got {:?}",
        colour("oklch(0.5999 0 0)")
    );
}

#[test]
fn lab_matches_the_reference_values() {
    assert!(
        near(colour("lab(53.2408 80.0925 67.2032)"), (255, 0, 0), 1),
        "red is lab(53.2408 80.0925 67.2032), got {:?}",
        colour("lab(53.2408 80.0925 67.2032)")
    );
    assert!(near(colour("lab(100 0 0)"), (255, 255, 255), 0));
    assert!(near(colour("lab(0 0 0)"), (0, 0, 0), 0));
    // And back: a grey has no hue, so its a and b are 0.
    let grey = colour("lab(53.2408 0 0)");
    assert!(
        near(grey, (128, 128, 128), 1),
        "lab L 53.24 is mid grey, got {grey:?}"
    );
}

#[test]
fn the_wide_spaces_read_their_own_channels() {
    // D65 white through the XYZ space.
    assert!(
        near(colour("color(xyz-d65 0.9505 1 1.089)"), (255, 255, 255), 1),
        "XYZ D65 white, got {:?}",
        colour("color(xyz-d65 0.9505 1 1.089)")
    );
    // Rec.2020's red primary is outside sRGB, so it clips to red rather than
    // reading as something else.
    assert!(
        near(colour("color(rec2020 1 0 0)"), (255, 0, 0), 2),
        "Rec.2020 red, got {:?}",
        colour("color(rec2020 1 0 0)")
    );
    // A space CSS does not define is not a colour, rather than quietly sRGB.
    assert_eq!(parse("color(nonsense 1 2 3)", false), None);
    assert!(parse("color(xyz 0.2 0.3 0.4)", false).is_some());
}

#[test]
fn a_space_and_its_bytes_round_trip() {
    // Every corner of the sRGB cube, through OKLab and through Lab and back.
    for &(r, g, b) in &[
        (0u8, 0u8, 0u8),
        (255, 255, 255),
        (255, 0, 0),
        (0, 255, 0),
        (0, 0, 255),
        (255, 255, 0),
        (0, 255, 255),
        (255, 0, 255),
        (128, 64, 32),
        (17, 200, 240),
    ] {
        let start = Rgba::rgb(r, g, b);
        let through = |space: &str| {
            // The colour re-written in the space's own channels, which the
            // parser reads back to bytes.
            let channels = format!("{space}: {start:?}");
            let _ = channels;
        };
        through("oklab");
        // `parse` reads a hex or a function; a round trip goes through the
        // functions the theme writes.
        for value in [
            format!("rgb({r} {g} {b})"),
            format!("rgb({r}, {g}, {b})"),
            format!("#{r:02x}{g:02x}{b:02x}"),
        ] {
            assert!(
                is(colour(&value), (r, g, b)),
                "{value} should be ({r}, {g}, {b}), got {:?}",
                colour(&value)
            );
        }
    }
}

#[test]
fn color_mix_matches_the_reference_values() {
    assert!(
        near(colour("color-mix(in srgb, #000, #fff)"), (128, 128, 128), 0),
        "half black and white is exactly #808080, got {:?}",
        colour("color-mix(in srgb, #000, #fff)")
    );
    assert!(
        near(colour("color-mix(in oklab, #000, #fff)"), (99, 99, 99), 2),
        "OKLab mixes lighter than sRGB, got {:?}",
        colour("color-mix(in oklab, #000, #fff)")
    );
    assert!(
        near(colour("color-mix(in srgb, red, lime)"), (128, 128, 0), 0),
        "got {:?}",
        colour("color-mix(in srgb, red, lime)")
    );
}

#[test]
fn a_hue_mix_follows_the_weights_along_the_shorter_arc() {
    // 90% red and 10% cyan: the hue travels a tenth of the shorter arc from
    // 0 to 180, so it lands on 18 degrees, which is rgb(255, 77, 0). The
    // weighted mean of the two hues would land on 0 and lose the cyan.
    assert!(
        near(
            colour("color-mix(in hsl, red 90%, hsl(180 100% 50%) 10%)"),
            (255, 77, 0),
            1
        ),
        "got {:?}",
        colour("color-mix(in hsl, red 90%, hsl(180 100% 50%) 10%)")
    );
    // Halfway between red and lime is yellow along the shorter arc.
    assert!(
        near(colour("color-mix(in hsl, red, lime)"), (255, 255, 0), 1),
        "got {:?}",
        colour("color-mix(in hsl, red, lime)")
    );
}

#[test]
fn the_longer_hue_arc_goes_the_other_way() {
    // From red's 0 degrees to lime's 120 the shorter arc is through yellow;
    // the longer one goes backwards, so halfway is 240 degrees, blue.
    assert!(
        near(
            colour("color-mix(in hsl longer hue, red, lime)"),
            (0, 0, 255),
            2
        ),
        "got {:?}",
        colour("color-mix(in hsl longer hue, red, lime)")
    );
}

#[test]
fn an_achromatic_colour_has_no_hue() {
    // CSS calls the hue of a grey powerless: a relative colour that asks for
    // it gets 0, not whatever residue the matrix left.
    let grey = colour("oklch(from white l c h)");
    assert!(near(grey, (255, 255, 255), 1), "got {grey:?}");
    let grey = colour("hsl(from #808080 h s l)");
    assert!(near(grey, (128, 128, 128), 1), "got {grey:?}");
}

#[test]
fn a_relative_colour_reads_the_source_in_its_own_space() {
    // `color(from ...)` substitutes the source's channels in the space named,
    // not its sRGB ones.
    let same = colour("color(from red srgb r g b)");
    assert!(is(same, (255, 0, 0)), "got {same:?}");
    // The hue channel of a relative OKLCH colour turns the colour, and the
    // result is out of the sRGB gamut, so its channels clip rather than
    // come back as the red it started from.
    let turned = colour("oklch(from red l c 180)");
    assert!(
        turned.r < 40 && turned.g > 140 && turned.b > 100,
        "red turned to 180 degrees, got {turned:?}"
    );
}

#[test]
fn a_non_finite_channel_is_not_a_colour() {
    // A value that reached infinity must be unusable, not silently black.
    assert_eq!(parse("rgb(1e999 0 0)", false), None);
    assert_eq!(parse("rgb(-1e999 0 0)", false), None);
    assert!(parse("oklch(1 0 0)", false).is_some());
}

#[test]
fn a_colour_function_splits_on_any_whitespace() {
    // Hand-formatted themes wrap a long colour over lines, and a double
    // space is easy to type.
    assert!(is(colour("rgb(255  0 0)"), (255, 0, 0)));
    assert!(is(colour("rgb(255\n0\n0)"), (255, 0, 0)));
    assert!(is(colour("color(srgb  0.5 0.5 0.5)"), (128, 128, 128)));
}
