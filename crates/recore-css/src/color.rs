//! The colours a Discord theme's CSS names, read into RGBA without pulling in
//! a browser engine. Hex, `rgb()`, `rgba()`, `hsl()`, `hsla()`, `hwb()`,
//! `oklab()`, `oklch()`, `lab()`, `lch()`, `color()`, named colours,
//! `transparent`, `color-mix()`, `light-dark()`, `contrast-color()` and the
//! relative `rgb(from ...)` family are understood; anything else is left to
//! the caller's fallback.

use std::fmt::Write as _;

/// A colour with straight (not premultiplied) alpha, as CSS works them out.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct Rgba {
    /// The red channel, 0 to 255.
    pub r: u8,
    /// The green channel, 0 to 255.
    pub g: u8,
    /// The blue channel, 0 to 255.
    pub b: u8,
    /// The alpha channel, 0 (clear) to 255 (opaque).
    pub a: u8,
}

impl Rgba {
    /// A clear black, which paints nothing.
    pub const TRANSPARENT: Self = Self {
        r: 0,
        g: 0,
        b: 0,
        a: 0,
    };

    /// An opaque colour from its red, green and blue channels.
    pub const fn rgb(r: u8, g: u8, b: u8) -> Self {
        Self { r, g, b, a: 255 }
    }

    /// A colour from all four channels, as CSS `rgba()` writes them.
    #[allow(clippy::self_named_constructors, reason = "rgba is the CSS name")]
    pub const fn rgba(r: u8, g: u8, b: u8, a: u8) -> Self {
        Self { r, g, b, a }
    }

    /// The same colour at another alpha, 0 to 255.
    pub const fn with_alpha(self, a: u8) -> Self {
        Self { a, ..self }
    }

    /// The colour's brightness, 0 (black) to 1 (white), as WCAG weights it.
    pub fn luminance(self) -> f32 {
        let channel = |value: u8| {
            let value = f32::from(value) / 255.0;
            if value <= 0.040_45 {
                value / 12.92
            } else {
                ((value + 0.055) / 1.055).powf(2.4)
            }
        };
        0.2126 * channel(self.r) + 0.7152 * channel(self.g) + 0.0722 * channel(self.b)
    }

    /// Whether the colour reads dark on a screen.
    pub fn is_dark(self) -> bool {
        self.luminance() < 0.5
    }

    /// `self` over `background`, as a browser paints a translucent layer.
    pub fn over(self, background: Self) -> Self {
        if self.a == 255 {
            return self;
        }
        if self.a == 0 {
            return background;
        }
        let source = f32::from(self.a) / 255.0;
        let back = f32::from(background.a) / 255.0;
        let alpha = source + back * (1.0 - source);
        if alpha <= f32::EPSILON {
            return Self::TRANSPARENT;
        }
        let mix = |front: u8, behind: u8| {
            let value =
                (f32::from(front) * source + f32::from(behind) * back * (1.0 - source)) / alpha;
            value.round().clamp(0.0, 255.0) as u8
        };
        Self {
            r: mix(self.r, background.r),
            g: mix(self.g, background.g),
            b: mix(self.b, background.b),
            a: (alpha * 255.0).round().clamp(0.0, 255.0) as u8,
        }
    }

    /// The WCAG contrast ratio between two colours, for picking text that reads.
    pub fn contrast(self, other: Self) -> f32 {
        let (a, b) = (self.luminance(), other.luminance());
        (a.max(b) + 0.05) / (a.min(b) + 0.05)
    }

    /// Black or white, whichever reads better on this colour.
    pub fn ink(self) -> Self {
        if self.luminance() > 0.45 {
            Self::rgb(0, 0, 0)
        } else {
            Self::rgb(255, 255, 255)
        }
    }

    /// `self` with `amount` (0..1) of `other` mixed in, as CSS `color-mix`
    /// mixes in sRGB with premultiplied alpha.
    pub fn mix(self, other: Self, amount: f32) -> Self {
        // A share that is not a number mixes nothing, rather than a colour
        // whose channels are all lost.
        if !amount.is_finite() {
            return self;
        }
        let amount = amount.clamp(0.0, 1.0);
        if amount <= 0.0 {
            return self;
        }
        if amount >= 1.0 {
            return other;
        }
        // Opaque colours, as most gradients are, skip the alpha weighting
        // and the division per channel.
        if self.a == 255 && other.a == 255 {
            let channel = |a: u8, b: u8| {
                (f32::from(a) + (f32::from(b) - f32::from(a)) * amount).round() as u8
            };
            return Self {
                r: channel(self.r, other.r),
                g: channel(self.g, other.g),
                b: channel(self.b, other.b),
                a: 255,
            };
        }
        let first = f32::from(self.a) / 255.0 * (1.0 - amount);
        let second = f32::from(other.a) / 255.0 * amount;
        let total = first + second;
        if total <= f32::EPSILON {
            return Self::TRANSPARENT;
        }
        let channel = |a: u8, b: u8| {
            ((f32::from(a) * first + f32::from(b) * second) / total)
                .round()
                .clamp(0.0, 255.0) as u8
        };
        Self {
            r: channel(self.r, other.r),
            g: channel(self.g, other.g),
            b: channel(self.b, other.b),
            a: ((total * 255.0).round().clamp(0.0, 255.0)) as u8,
        }
    }

    /// The colour as `#rrggbb`, or `#rrggbbaa` when it isn't opaque.
    pub fn hex(self) -> String {
        if self.a == 255 {
            format!("#{:02x}{:02x}{:02x}", self.r, self.g, self.b)
        } else {
            format!("#{:02x}{:02x}{:02x}{:02x}", self.r, self.g, self.b, self.a)
        }
    }
}

/// Parses the colour a CSS declaration uses, with `light` choosing which of
/// `light-dark()`'s two colours applies: its first in light mode, its second
/// in dark mode.
pub fn parse(text: &str, light: bool) -> Option<Rgba> {
    parse_depth(text, light, 0)
}

/// How deep a colour expression may nest `color-mix()` or `light-dark()`.
/// A damaged theme can repeat them thousands of times; the window reads it
/// as a colour it can't use instead of running out of stack.
const MAX_DEPTH: usize = 32;

/// How deeply a colour's arithmetic may nest, so a hostile value cannot
/// exhaust the stack: the parentheses of `calc()` and the functions inside
/// it.
const MAX_ARITHMETIC_DEPTH: usize = 32;

/// The chroma below which an OKLCh colour has no hue: white and grey leave a
/// residue of about 1e-8 in the conversion, and CSS calls their hue
/// powerless.
const MIN_OK_CHROMA: f32 = 0.000_004;

/// The chroma below which a CIE LCh colour has no hue, as CSS's epsilon.
const MIN_LAB_CHROMA: f32 = 0.001_5;

fn parse_depth(text: &str, light: bool, depth: usize) -> Option<Rgba> {
    if depth > MAX_DEPTH {
        return None;
    }
    let text = text.trim();
    if text.is_empty() {
        return None;
    }
    let lower = text.to_ascii_lowercase();
    match lower.as_str() {
        "transparent" => return Some(Rgba::TRANSPARENT),
        "black" => return Some(Rgba::rgb(0, 0, 0)),
        "white" => return Some(Rgba::rgb(255, 255, 255)),
        _ => {}
    }
    if let Some(hex) = lower.strip_prefix('#') {
        return parse_hex(hex);
    }
    if let Some(open) = text.find('(')
        && let Some(inside) = function_body(text)
    {
        let name = text[..open].trim().to_ascii_lowercase();
        let inside = inside.trim_start();
        if inside
            .get(..5)
            .is_some_and(|start| start.eq_ignore_ascii_case("from "))
        {
            return parse_relative(&name, inside, light, depth + 1);
        }
    }
    if lower.starts_with("contrast-color(") {
        let inside = function_body(text)?;
        return parse_contrast_color(inside, light, depth);
    }
    if lower.starts_with("rgb(") || lower.starts_with("rgba(") {
        let inside = function_body(text)?;
        return parse_rgb(inside);
    }
    if lower.starts_with("hsl(") || lower.starts_with("hsla(") {
        let inside = function_body(text)?;
        return parse_hsl(inside);
    }
    if lower.starts_with("hwb(") {
        let inside = function_body(text)?;
        return parse_hwb(inside);
    }
    if lower.starts_with("oklab(") {
        let inside = function_body(text)?;
        return parse_oklab(inside);
    }
    if lower.starts_with("oklch(") {
        let inside = function_body(text)?;
        return parse_oklch(inside);
    }
    if lower.starts_with("lab(") {
        let inside = function_body(text)?;
        return parse_lab(inside);
    }
    if lower.starts_with("lch(") {
        let inside = function_body(text)?;
        return parse_lch(inside);
    }
    if lower.starts_with("color(") {
        let inside = function_body(text)?;
        return parse_color(inside);
    }
    if lower.starts_with("color-mix(") {
        let inside = function_body(text)?;
        return parse_color_mix(inside, light, depth + 1);
    }
    if lower.starts_with("light-dark(") {
        let inside = function_body(text)?;
        let parts = split_top(inside, ',');
        let chosen = if light { parts.first() } else { parts.get(1) };
        return chosen.and_then(|part| parse_depth(part, light, depth + 1));
    }
    if let Some(rgba) = parse_triplet(&lower) {
        return Some(rgba);
    }
    named(&lower).map(|(r, g, b)| Rgba::rgb(r, g, b))
}

/// `30,30,30` or `30, 30, 30, 0.5`: DiscordRecolor keeps colours as the
/// three numbers a `rgb()` would take, and themes use them through
/// `rgb(var(--name))`.
fn parse_triplet(text: &str) -> Option<Rgba> {
    if !text.contains(',') {
        return None;
    }
    let parts = split_top(text, ',');
    if !(3..=4).contains(&parts.len()) {
        return None;
    }
    let channel = |text: &str| -> Option<u8> {
        let value = numeric(text)?;
        if value.angle {
            return None;
        }
        let value = if value.percent {
            value.value / 1e+02 * 255.0
        } else {
            value.value
        };
        Some(value.clamp(0.0, 255.0).round() as u8)
    };
    let alpha = match parts.get(3) {
        Some(alpha) => {
            let value = numeric(alpha)?;
            if value.angle {
                return None;
            }
            let value = if value.percent {
                value.value / 1e+02
            } else {
                value.value
            };
            if value <= 1.0 {
                (value.clamp(0.0, 1.0) * 255.0).round() as u8
            } else {
                value.clamp(0.0, 255.0).round() as u8
            }
        }
        None => 255,
    };
    Some(Rgba::rgba(
        channel(parts[0])?,
        channel(parts[1])?,
        channel(parts[2])?,
        alpha,
    ))
}

fn parse_hex(digits: &str) -> Option<Rgba> {
    if !digits.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return None;
    }
    let value = u32::from_str_radix(digits, 16).ok()?;
    match digits.len() {
        3 => {
            let nibble = |shift: u32| ((value >> shift) & 0xf) * 0x11;
            Some(Rgba::rgb(nibble(8) as u8, nibble(4) as u8, nibble(0) as u8))
        }
        4 => {
            let nibble = |shift: u32| ((value >> shift) & 0xf) * 0x11;
            Some(Rgba::rgba(
                nibble(12) as u8,
                nibble(8) as u8,
                nibble(4) as u8,
                nibble(0) as u8,
            ))
        }
        6 => Some(Rgba::rgb(
            ((value >> 16) & 0xff) as u8,
            ((value >> 8) & 0xff) as u8,
            (value & 0xff) as u8,
        )),
        8 => Some(Rgba::rgba(
            ((value >> 24) & 0xff) as u8,
            ((value >> 16) & 0xff) as u8,
            ((value >> 8) & 0xff) as u8,
            (value & 0xff) as u8,
        )),
        _ => None,
    }
}

/// The text between a function's name and its closing parenthesis.
fn function_body(text: &str) -> Option<&str> {
    let start = text.find('(')?;
    let end = matching_close(text, start)?;
    (end > start).then(|| &text[start + 1..end])
}

/// The closing parenthesis of the one at `open`, ignoring parentheses inside
/// strings.
fn matching_close(text: &str, open: usize) -> Option<usize> {
    let mut depth = 0u32;
    let mut quote: Option<char> = None;
    let mut escaped = false;
    for (index, character) in text.char_indices().skip_while(|(index, _)| *index < open) {
        if escaped {
            escaped = false;
            continue;
        }
        match character {
            '\\' => escaped = true,
            '"' | '\'' if quote == Some(character) => quote = None,
            '"' | '\'' if quote.is_none() => quote = Some(character),
            '(' if quote.is_none() => depth += 1,
            ')' if quote.is_none() => {
                depth = depth.checked_sub(1)?;
                if depth == 0 {
                    return Some(index);
                }
            }
            _ => {}
        }
    }
    None
}

/// Splits at `separator`, ignoring separators inside parentheses or strings.
pub fn split_top(text: &str, separator: char) -> Vec<&str> {
    let mut parts = Vec::new();
    let mut depth = 0u32;
    let mut quote: Option<char> = None;
    let mut escaped = false;
    let mut start = 0;
    for (index, character) in text.char_indices() {
        if escaped {
            escaped = false;
            continue;
        }
        match character {
            '\\' => escaped = true,
            '"' | '\'' if quote == Some(character) => quote = None,
            '"' | '\'' if quote.is_none() => quote = Some(character),
            '(' if quote.is_none() => depth += 1,
            ')' if quote.is_none() => depth = depth.saturating_sub(1),
            '{' if quote.is_none() => depth += 1,
            '}' if quote.is_none() => depth = depth.saturating_sub(1),
            _ if character == separator && quote.is_none() && depth == 0 => {
                parts.push(text[start..index].trim());
                start = index + character.len_utf8();
            }
            _ => {}
        }
    }
    parts.push(text[start..].trim());
    parts
}

/// Splits at `separator`, ignoring separators inside parentheses or strings,
/// and drops the parts that are empty. A whitespace separator splits at a
/// run of whitespace, as CSS's space-separated syntax does: `rgb(255  0 0)`
/// and `rgb(255\n0\n0)` are the same three values.
pub fn split_top_skippable(text: &str, separator: char) -> Vec<&str> {
    let mut parts = Vec::new();
    let mut depth = 0u32;
    let mut quote: Option<char> = None;
    let mut escaped = false;
    let mut start = 0;
    for (index, character) in text.char_indices() {
        if escaped {
            escaped = false;
            continue;
        }
        match character {
            '\\' => escaped = true,
            '"' | '\'' if quote == Some(character) => quote = None,
            '"' | '\'' if quote.is_none() => quote = Some(character),
            '(' if quote.is_none() => depth += 1,
            ')' if quote.is_none() => depth = depth.saturating_sub(1),
            '{' if quote.is_none() => depth += 1,
            '}' if quote.is_none() => depth = depth.saturating_sub(1),
            _ if quote.is_none() && depth == 0 && separates(character, separator) => {
                parts.push(text[start..index].trim());
                start = index + character.len_utf8();
            }
            _ => {}
        }
    }
    parts.push(text[start..].trim());
    parts.into_iter().filter(|part| !part.is_empty()).collect()
}

/// Whether a character separates two values: a whitespace separator splits
/// at a run of whitespace, as CSS's space-separated syntax does.
fn separates(character: char, separator: char) -> bool {
    if separator.is_whitespace() {
        character.is_whitespace()
    } else {
        character == separator
    }
}

/// The place a character sits at the top of a colour function's body,
/// outside its parentheses and strings.
fn split_at_top(text: &str, separator: char) -> Option<(&str, &str)> {
    let mut depth = 0u32;
    let mut quote: Option<char> = None;
    let mut escaped = false;
    for (index, character) in text.char_indices() {
        if escaped {
            escaped = false;
            continue;
        }
        match character {
            '\\' => escaped = true,
            '"' | '\'' if quote == Some(character) => quote = None,
            '"' | '\'' if quote.is_none() => quote = Some(character),
            '(' if quote.is_none() => depth += 1,
            ')' if quote.is_none() => depth = depth.saturating_sub(1),
            _ if character == separator && quote.is_none() && depth == 0 => {
                return Some((&text[..index], &text[index + character.len_utf8()..]));
            }
            _ => {}
        }
    }
    None
}

/// A number from a colour expression: a plain number, a percentage, an
/// angle, or a `calc()`/`min()`/`max()`/`clamp()` that settles to one.
#[derive(Clone, Copy, Debug, PartialEq)]
struct Numeric {
    value: f32,
    percent: bool,
    angle: bool,
}

impl Numeric {
    fn add(self, other: Self) -> Option<Self> {
        if self.percent == other.percent && self.angle == other.angle {
            let value = self.value + other.value;
            return value.is_finite().then_some(Self { value, ..self });
        }
        // CSS allows a unitless zero beside a percentage or an angle.
        if self.value == 0.0 && !self.percent && !self.angle {
            return Some(other);
        }
        if other.value == 0.0 && !other.percent && !other.angle {
            return Some(self);
        }
        None
    }

    fn sub(self, other: Self) -> Option<Self> {
        self.add(Self {
            value: -other.value,
            ..other
        })
    }

    fn mul(self, other: Self) -> Option<Self> {
        if (self.percent || self.angle) && (other.percent || other.angle) {
            return None;
        }
        let value = self.value * other.value;
        value.is_finite().then_some(Self {
            value,
            percent: self.percent || other.percent,
            angle: self.angle || other.angle,
        })
    }

    fn div(self, other: Self) -> Option<Self> {
        if other.percent || other.angle || other.value == 0.0 {
            return None;
        }
        let value = self.value / other.value;
        value.is_finite().then_some(Self { value, ..self })
    }
}

/// Evaluates a numeric CSS value, including `calc()` and its relatives.
fn numeric(text: &str) -> Option<Numeric> {
    numeric_depth(text, 0)
}

/// Evaluates a numeric CSS value, `depth` levels of arithmetic deep.
fn numeric_depth(text: &str, depth: usize) -> Option<Numeric> {
    if depth > MAX_ARITHMETIC_DEPTH {
        return None;
    }
    let text = text.trim();
    if text.is_empty() || !text.is_ascii() {
        return None;
    }
    if let Some((name, inside)) = whole_function(text) {
        return match name.as_str() {
            "calc" => Parser::new(inside, depth + 1).all(),
            "min" | "max" => {
                let mut values = crate::color::split_top(inside, ',')
                    .into_iter()
                    .map(|part| numeric_depth(part, depth + 1))
                    .collect::<Option<Vec<_>>>()?;
                let first = *values.first()?;
                if values
                    .iter()
                    .any(|value| value.percent != first.percent || value.angle != first.angle)
                {
                    return None;
                }
                values.sort_by(|left, right| left.value.total_cmp(&right.value));
                Some(if name == "min" {
                    values[0]
                } else {
                    values[values.len() - 1]
                })
            }
            "clamp" => {
                let values = crate::color::split_top(inside, ',')
                    .into_iter()
                    .map(|part| numeric_depth(part, depth + 1))
                    .collect::<Option<Vec<_>>>()?;
                if values.len() != 3
                    || values.iter().any(|value| {
                        value.percent != values[0].percent || value.angle != values[0].angle
                    })
                {
                    return None;
                }
                let value = values[1].value.max(values[0].value).min(values[2].value);
                Some(Numeric { value, ..values[1] })
            }
            "abs" => {
                let mut value = numeric_depth(inside, depth + 1)?;
                value.value = value.value.abs();
                Some(value)
            }
            _ => None,
        };
    }
    literal(text)
}

fn literal(text: &str) -> Option<Numeric> {
    let text = text.trim();
    let bytes = text.as_bytes();
    let mut index = 0;
    if bytes.get(index) == Some(&b'.') {
        index += 1;
    }
    while bytes.get(index).is_some_and(u8::is_ascii_digit) {
        index += 1;
    }
    if bytes.get(index) == Some(&b'.') {
        index += 1;
        while bytes.get(index).is_some_and(u8::is_ascii_digit) {
            index += 1;
        }
    }
    if !bytes
        .get(..index)
        .is_some_and(|number| number.iter().any(|byte| byte.is_ascii_digit()))
    {
        return None;
    }
    if matches!(bytes.get(index), Some(b'e' | b'E')) {
        let exponent = index;
        index += 1;
        if matches!(bytes.get(index), Some(b'+' | b'-')) {
            index += 1;
        }
        let digits = index;
        while bytes.get(index).is_some_and(u8::is_ascii_digit) {
            index += 1;
        }
        if digits == index {
            index = exponent;
        }
    }
    let value: f32 = text[..index].parse().ok()?;
    // A number too large to hold is not a colour.
    if !value.is_finite() {
        return None;
    }
    let unit = text[index..].trim();
    let (percent, angle, value) = match unit.to_ascii_lowercase().as_str() {
        "" => (false, false, value),
        "%" => (true, false, value),
        "deg" => (false, true, value),
        "grad" => (false, true, value * 0.9),
        "rad" => (false, true, value.to_degrees()),
        "turn" => (false, true, value * 3.6e+02),
        _ => return None,
    };
    if !value.is_finite() {
        return None;
    }
    Some(Numeric {
        value,
        percent,
        angle,
    })
}

/// The name and inside of `name(...)` when it covers the whole text.
fn whole_function(text: &str) -> Option<(String, &str)> {
    let open = text.find('(')?;
    let name = text[..open].trim().to_ascii_lowercase();
    let close = matching_close(text, open)?;
    (close + 1 == text.len()).then(|| (name, &text[open + 1..close]))
}

/// A tiny arithmetic parser for the numbers inside colour functions.
struct Parser<'a> {
    text: &'a [u8],
    at: usize,
    /// How many parentheses deep the parser is, so a hostile value cannot
    /// run the window out of stack.
    depth: usize,
}

impl<'a> Parser<'a> {
    fn new(text: &'a str, depth: usize) -> Self {
        Self {
            text: text.as_bytes(),
            at: 0,
            depth,
        }
    }

    fn all(mut self) -> Option<Numeric> {
        let value = self.sum()?;
        self.space();
        (self.at == self.text.len()).then_some(value)
    }

    fn space(&mut self) {
        while self.text.get(self.at).is_some_and(u8::is_ascii_whitespace) {
            self.at += 1;
        }
    }

    fn eat(&mut self, byte: u8) -> bool {
        self.space();
        if self.text.get(self.at) == Some(&byte) {
            self.at += 1;
            true
        } else {
            false
        }
    }

    fn sum(&mut self) -> Option<Numeric> {
        let mut value = self.product()?;
        loop {
            if self.eat(b'+') {
                value = value.add(self.product()?)?;
            } else if self.eat(b'-') {
                value = value.sub(self.product()?)?;
            } else {
                return Some(value);
            }
        }
    }

    fn product(&mut self) -> Option<Numeric> {
        let mut value = self.unary()?;
        loop {
            if self.eat(b'*') {
                value = value.mul(self.unary()?)?;
            } else if self.eat(b'/') {
                value = value.div(self.unary()?)?;
            } else {
                return Some(value);
            }
        }
    }

    fn unary(&mut self) -> Option<Numeric> {
        // The sign is read in a loop, not in a recursion: a value with a
        // megabyte of `+` signs must not reach a million stack frames.
        let mut sign = 1.0f32;
        loop {
            self.space();
            match self.text.get(self.at) {
                Some(b'+') => self.at += 1,
                Some(b'-') => {
                    self.at += 1;
                    sign = -sign;
                }
                _ => break,
            }
        }
        let mut value = self.primary()?;
        if sign < 0.0 {
            value.value = -value.value;
        }
        Some(value)
    }

    fn primary(&mut self) -> Option<Numeric> {
        self.space();
        if self.eat(b'(') {
            if self.depth >= MAX_ARITHMETIC_DEPTH {
                return None;
            }
            self.depth += 1;
            let value = self.sum();
            self.depth -= 1;
            let value = value?;
            return self.eat(b')').then_some(value);
        }
        let start = self.at;
        while self.text.get(self.at).is_some_and(u8::is_ascii_alphabetic) {
            self.at += 1;
        }
        if self.text.get(self.at) == Some(&b'(') {
            let inside = self.at + 1;
            let mut depth = 1u32;
            let mut end = inside;
            while let Some(&byte) = self.text.get(end) {
                match byte {
                    b'(' => depth += 1,
                    b')' => {
                        depth -= 1;
                        if depth == 0 {
                            break;
                        }
                    }
                    _ => {}
                }
                end += 1;
            }
            if depth != 0 {
                return None;
            }
            let function = std::str::from_utf8(&self.text[start..=end]).ok()?;
            self.at = end + 1;
            return numeric_depth(function, self.depth + 1);
        }
        let mut end = self.at;
        while self
            .text
            .get(end)
            .is_some_and(|byte| !byte.is_ascii_whitespace() && !b"()+-*/,".contains(byte))
        {
            end += 1;
        }
        if start == end {
            return None;
        }
        let token = std::str::from_utf8(&self.text[start..end]).ok()?;
        self.at = end;
        literal(token)
    }
}

/// `255`, `50%` or `1` (alpha), as an rgb component.
fn component(text: &str, percent_is_full: bool) -> Option<f32> {
    let value = numeric(text)?;
    if value.angle {
        return None;
    }
    Some(if value.percent {
        value.value / 1e+02 * if percent_is_full { 1.0 } else { 255.0 }
    } else {
        value.value
    })
}

fn alpha_component(text: &str) -> Option<u8> {
    let value = component(text, true)?;
    Some((value.clamp(0.0, 1.0) * 255.0).round() as u8)
}

/// The channels of a colour function's body, and its alpha when a solidus
/// separates it.
fn split_alpha(inside: &str) -> (&str, Option<&str>) {
    match split_at_top(inside, '/') {
        Some((components, alpha)) => (components.trim(), Some(alpha.trim())),
        None => (inside.trim(), None),
    }
}

/// Whether a colour function's body uses the legacy comma syntax.
fn is_legacy(components: &str) -> bool {
    split_at_top(components, ',').is_some()
}

/// `rgb(1 2 3 / 50%)`, `rgba(1, 2, 3, .5)` and the percentages between.
fn parse_rgb(inside: &str) -> Option<Rgba> {
    let inside = inside.trim();
    let (components, alpha) = split_alpha(inside);
    let parts = if is_legacy(components) {
        split_top(components, ',')
    } else {
        split_top_skippable(components, ' ')
    };
    // Three channels and an alpha, and nothing else.
    if !(3..=4).contains(&parts.len()) {
        return None;
    }
    let channel = |text: &str| -> Option<u8> {
        Some(component(text, false)?.clamp(0.0, 255.0).round() as u8)
    };
    let a = match alpha {
        Some(alpha) => alpha_component(alpha)?,
        None if parts.len() == 4 => alpha_component(parts[3])?,
        None => 255,
    };
    Some(Rgba::rgba(
        channel(parts[0])?,
        channel(parts[1])?,
        channel(parts[2])?,
        a,
    ))
}

/// `hsl(120 100% 25% / 50%)` and the comma syntax.
fn parse_hsl(inside: &str) -> Option<Rgba> {
    let inside = inside.trim();
    let (components, alpha) = split_alpha(inside);
    let legacy = is_legacy(components);
    let parts = if legacy {
        split_top(components, ',')
    } else {
        split_top_skippable(components, ' ')
    };
    if !(3..=4).contains(&parts.len()) {
        return None;
    }
    let hue = hue(parts[0])?;
    let saturation = portion(parts[1], legacy)?;
    let lightness = portion(parts[2], legacy)?;
    let a = alpha_value(alpha, parts.get(3).copied())?;
    let (red, green, blue) = hsl_rgb(hue, saturation, lightness);
    hsl_bytes([red, green, blue], a)
}

/// `hwb(120 10% 20% / 50%)`.
fn parse_hwb(inside: &str) -> Option<Rgba> {
    let inside = inside.trim();
    let (components, alpha) = split_alpha(inside);
    let legacy = is_legacy(components);
    let parts = if legacy {
        split_top(components, ',')
    } else {
        split_top_skippable(components, ' ')
    };
    if !(3..=4).contains(&parts.len()) {
        return None;
    }
    let hue = hue(parts[0])?;
    let whiteness = portion(parts[1], legacy)?;
    let blackness = portion(parts[2], legacy)?;
    let a = alpha_value(alpha, parts.get(3).copied())?;
    let (red, green, blue) = hwb_rgb(hue, whiteness, blackness);
    hsl_bytes([red, green, blue], a)
}

/// `oklab(0.62 0.04 -0.03 / 50%)`, as Material themes write colours.
fn parse_oklab(inside: &str) -> Option<Rgba> {
    let inside = inside.trim();
    let (components, alpha) = split_alpha(inside);
    let parts = if is_legacy(components) {
        split_top(components, ',')
    } else {
        split_top_skippable(components, ' ')
    };
    if !(3..=4).contains(&parts.len()) {
        return None;
    }
    let lightness = number_or_percent(parts[0], 1.0)?;
    let a = signed(parts[1], 0.4)?;
    let b = signed(parts[2], 0.4)?;
    let alpha = alpha_value(alpha, parts.get(3).copied())?;
    oklab_rgba(lightness, a, b, alpha)
}

/// `oklch(0.62 0.12 30 / 50%)`.
fn parse_oklch(inside: &str) -> Option<Rgba> {
    let inside = inside.trim();
    let (components, alpha) = split_alpha(inside);
    let parts = if is_legacy(components) {
        split_top(components, ',')
    } else {
        split_top_skippable(components, ' ')
    };
    if !(3..=4).contains(&parts.len()) {
        return None;
    }
    let lightness = number_or_percent(parts[0], 1.0)?;
    let chroma = number_or_percent(parts[1], 0.4)?;
    let hue = hue(parts[2])?;
    let alpha = alpha_value(alpha, parts.get(3).copied())?;
    let (a, b) = (
        chroma * (hue.to_radians()).cos(),
        chroma * (hue.to_radians()).sin(),
    );
    oklab_rgba(lightness, a, b, alpha)
}

fn oklab_rgba(lightness: f32, a: f32, b: f32, alpha: u8) -> Option<Rgba> {
    let l = (lightness + 0.396_337_78 * a + 0.215_803_76 * b).powi(3);
    let m = (lightness - 0.105_561_346 * a - 0.063_854_17 * b).powi(3);
    let s = (lightness - 0.089_484_18 * a - 1.291_485_5 * b).powi(3);
    let red = 4.076_741_7 * l - 3.307_711_6 * m + 0.230_969_93 * s;
    let green = -1.268_438 * l + 2.609_757_4 * m - 0.341_319_4 * s;
    let blue = -0.0041960863 * l - 0.703_418_6 * m + 1.707_614_7 * s;
    srgb_bytes([red, green, blue], alpha)
}

/// The sRGB channel of a linear-light value, as CSS's transfer function
/// works it out. The threshold is 0.0031308.
fn srgb_encode(value: f32) -> Option<f32> {
    if !value.is_finite() {
        return None;
    }
    let value = value.clamp(0.0, 1.0);
    Some(if value <= 0.003_130_8 {
        12.92 * value
    } else {
        1.055 * value.powf(1.0 / 2.4) - 0.055
    })
}

fn srgb_decode(value: f32) -> f32 {
    if value <= 0.040_45 {
        value / 12.92
    } else {
        ((value + 0.055) / 1.055).powf(2.4)
    }
}

/// The linear-light value of an sRGB channel, as CSS's transfer function
/// works it out. The threshold is 0.04045.
pub fn srgb_to_linear(value: f32) -> f32 {
    srgb_decode(value)
}

/// The sRGB channel of a linear-light value, as CSS's transfer function
/// works it out. The threshold is 0.0031308.
pub fn linear_to_srgb(value: f32) -> f32 {
    srgb_encode(value).unwrap_or(f32::NAN)
}

/// `lab(53.24 80.09 67.2 / 50%)`, CIELAB with a D65 white point.
fn parse_lab(inside: &str) -> Option<Rgba> {
    let inside = inside.trim();
    let (components, alpha) = split_alpha(inside);
    let parts = if is_legacy(components) {
        split_top(components, ',')
    } else {
        split_top_skippable(components, ' ')
    };
    if !(3..=4).contains(&parts.len()) {
        return None;
    }
    let lightness = number_or_percent(parts[0], 1e+02)?;
    let a = signed(parts[1], 125.0)?;
    let b = signed(parts[2], 125.0)?;
    let alpha = alpha_value(alpha, parts.get(3).copied())?;
    lab_rgba(lightness, a, b, alpha)
}

/// `lch(53.24 104.55 40 / 50%)`, CIELAB's cylindrical form.
fn parse_lch(inside: &str) -> Option<Rgba> {
    let inside = inside.trim();
    let (components, alpha) = split_alpha(inside);
    let parts = if is_legacy(components) {
        split_top(components, ',')
    } else {
        split_top_skippable(components, ' ')
    };
    if !(3..=4).contains(&parts.len()) {
        return None;
    }
    let lightness = number_or_percent(parts[0], 1e+02)?;
    let chroma = number_or_percent(parts[1], 1.5e+02)?;
    let hue = hue(parts[2])?;
    let alpha = alpha_value(alpha, parts.get(3).copied())?;
    lab_rgba(
        lightness,
        chroma * hue.to_radians().cos(),
        chroma * hue.to_radians().sin(),
        alpha,
    )
}

fn lab_rgba(lightness: f32, a: f32, b: f32, alpha: u8) -> Option<Rgba> {
    let fy = (lightness + 16.0) / 116.0;
    let fx = fy + a / 5e+02;
    let fz = fy - b / 2e+02;
    let finv = |value: f32| {
        if value.powi(3) > 0.008_856 {
            value.powi(3)
        } else {
            (116.0 * value - 16.0) / 903.3
        }
    };
    let x = 0.950_47 * finv(fx);
    let y = finv(fy);
    let z = 1.088_83 * finv(fz);
    let red = 3.240_454_2 * x - 1.537_138_5 * y - 0.498_531_4 * z;
    let green = -0.969_266 * x + 1.876_010_8 * y + 0.041_556 * z;
    let blue = 0.055_643_4 * x - 0.204_025_9 * y + 1.057_225_2 * z;
    srgb_bytes([red, green, blue], alpha)
}

/// `color(srgb 0.5 0.2 0.1 / 50%)`, `color(display-p3 ...)` and the other
/// spaces CSS Color 4 defines. A space CSS does not define is not a colour.
fn parse_color(inside: &str) -> Option<Rgba> {
    let inside = inside.trim();
    let (components, alpha) = split_alpha(inside);
    let parts = split_top_skippable(components, ' ');
    if parts.len() != 4 {
        return None;
    }
    let space = ColorSpace::of(&parts[0].to_ascii_lowercase())?;
    let red = component(parts[1], true)?;
    let green = component(parts[2], true)?;
    let blue = component(parts[3], true)?;
    let alpha = alpha_value(alpha, None)?;
    srgb_bytes(space.to_linear([red, green, blue])?, alpha)
}

/// CSS Color 5's relative form: `rgb(from #123 r g b / 50%)`,
/// `oklch(from red l c h)`. The source is any colour the reader knows; each
/// channel may keep its source value by naming it (`r`, `g`, `b`, `h`, `s`,
/// `l`, `a`, `c`) or give a new number or percentage.
fn parse_relative(name: &str, inside: &str, light: bool, depth: usize) -> Option<Rgba> {
    if depth > MAX_DEPTH {
        return None;
    }
    let from = inside.trim_start().get(5..)?.trim_start();
    let source_text = *split_top_skippable(from, ' ').first()?;
    if source_text.is_empty() {
        return None;
    }
    let source = parse_depth(source_text, light, depth + 1)?;
    let tail = from.get(source_text.len()..).unwrap_or("").trim();
    let (components, alpha) = split_alpha(tail);
    let parts = split_top_skippable(components, ' ');
    // `color(from <source> <space> r g b)` names the space its channels are
    // read in; every other relative colour names its three channels alone.
    let (space, parts) = if name == "color" && parts.len() == 4 {
        (parts[0].trim().to_ascii_lowercase(), &parts[1..])
    } else {
        (String::new(), parts.as_slice())
    };
    // The three channels of the colour, and nothing else.
    if parts.len() != 3 {
        return None;
    }
    let alpha = match alpha {
        None => source.a,
        Some(token) if token.trim().eq_ignore_ascii_case("alpha") => source.a,
        Some(token) => alpha_component(&channel_text(
            token,
            &[("alpha", f32::from(source.a) / 255.0)],
        ))?,
    };
    let result = match name {
        "rgb" | "rgba" => {
            let channels = [
                ("r", f32::from(source.r)),
                ("g", f32::from(source.g)),
                ("b", f32::from(source.b)),
            ];
            let red = relative_number(parts[0], "r", 255.0, &channels)?;
            let green = relative_number(parts[1], "g", 255.0, &channels)?;
            let blue = relative_number(parts[2], "b", 255.0, &channels)?;
            Rgba::rgba(
                red.clamp(0.0, 255.0).round() as u8,
                green.clamp(0.0, 255.0).round() as u8,
                blue.clamp(0.0, 255.0).round() as u8,
                alpha,
            )
        }
        "hsl" | "hsla" => {
            let (hue0, sat0, light0) = rgb_to_hsl(source);
            let channels = [("h", hue0), ("s", sat0 * 1e+02), ("l", light0 * 1e+02)];
            let hue = relative_hue(parts[0], "h", hue0, &channels)?;
            let saturation = relative_fraction(parts[1], "s", sat0, &channels)?;
            let lightness = relative_fraction(parts[2], "l", light0, &channels)?;
            let (red, green, blue) = hsl_rgb(hue, saturation, lightness);
            hsl_bytes([red, green, blue], alpha)?
        }
        "hwb" => {
            let (hue0, white0, black0) = rgb_to_hwb(source);
            let channels = [("h", hue0), ("w", white0 * 1e+02), ("b", black0 * 1e+02)];
            let hue = relative_hue(parts[0], "h", hue0, &channels)?;
            let white = relative_fraction(parts[1], "w", white0, &channels)?;
            let black = relative_fraction(parts[2], "b", black0, &channels)?;
            let (red, green, blue) = hwb_rgb(hue, white, black);
            hsl_bytes([red, green, blue], alpha)?
        }
        "oklab" => {
            let (lightness0, a0, b0) = rgb_to_oklab(source);
            let channels = [("l", lightness0), ("a", a0), ("b", b0)];
            let lightness = relative_number(parts[0], "l", 1.0, &channels)?;
            let a = relative_signed(parts[1], "a", 0.4, &channels)?;
            let b = relative_signed(parts[2], "b", 0.4, &channels)?;
            oklab_rgba(lightness, a, b, alpha)?
        }
        "oklch" => {
            let (lightness0, chroma0, hue0) = to_oklch(source);
            let channels = [("l", lightness0), ("c", chroma0), ("h", hue0)];
            let lightness = relative_number(parts[0], "l", 1.0, &channels)?;
            let chroma = relative_number(parts[1], "c", 0.4, &channels)?;
            let hue = relative_hue(parts[2], "h", hue0, &channels)?;
            oklab_rgba(
                lightness,
                chroma * hue.to_radians().cos(),
                chroma * hue.to_radians().sin(),
                alpha,
            )?
        }
        "lab" => {
            let (lightness0, a0, b0) = rgb_to_lab(source);
            let channels = [("l", lightness0), ("a", a0), ("b", b0)];
            let lightness = relative_number(parts[0], "l", 1e+02, &channels)?;
            let a = relative_signed(parts[1], "a", 125.0, &channels)?;
            let b = relative_signed(parts[2], "b", 125.0, &channels)?;
            lab_rgba(lightness, a, b, alpha)?
        }
        "lch" => {
            let (lightness0, chroma0, hue0) = to_lch(source);
            let channels = [("l", lightness0), ("c", chroma0), ("h", hue0)];
            let lightness = relative_number(parts[0], "l", 1e+02, &channels)?;
            let chroma = relative_number(parts[1], "c", 1.5e+02, &channels)?;
            let hue = relative_hue(parts[2], "h", hue0, &channels)?;
            lab_rgba(
                lightness,
                chroma * hue.to_radians().cos(),
                chroma * hue.to_radians().sin(),
                alpha,
            )?
        }
        "color" => {
            // The source's channels in the space the colour names, as CSS
            // substitutes them.
            let space = ColorSpace::of(&space)?;
            let source_channels = space.channels_from_linear(linear_rgb(source))?;
            let names = space.channel_names();
            let named: [(&str, f32); 3] = [
                (names[0], source_channels[0]),
                (names[1], source_channels[1]),
                (names[2], source_channels[2]),
            ];
            let mut channels = source_channels;
            for (index, keyword) in names.iter().enumerate() {
                channels[index] = relative_number(parts[index], keyword, 1.0, &named)?;
            }
            srgb_bytes(space.to_linear(channels)?, alpha)?
        }
        _ => return None,
    };
    Some(result)
}

/// Replaces the source channel keywords in a relative colour's channel,
/// including inside `calc()`: `calc(1 * s)` reads the source saturation.
fn channel_text(token: &str, channels: &[(&str, f32)]) -> String {
    let mut out = String::with_capacity(token.len());
    let bytes = token.as_bytes();
    let mut index = 0;
    while index < bytes.len() {
        let byte = bytes[index];
        if byte.is_ascii_alphabetic() || matches!(byte, b'_' | b'-') {
            let start = index;
            index += 1;
            while bytes
                .get(index)
                .is_some_and(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-'))
            {
                index += 1;
            }
            let word = &token[start..index];
            if let Some((_, value)) = channels
                .iter()
                .find(|(keyword, _)| word.eq_ignore_ascii_case(keyword))
                .filter(|(_, value)| value.is_finite())
            {
                let _ = write!(out, "{value}");
            } else {
                out.push_str(word);
            }
        } else if let Some(character) = token[index..].chars().next() {
            out.push(character);
            index += character.len_utf8();
        } else {
            break;
        }
    }
    out
}

fn relative_number(
    token: &str,
    _keyword: &str,
    full: f32,
    channels: &[(&str, f32)],
) -> Option<f32> {
    number_or_percent(&channel_text(token, channels), full)
}

fn relative_signed(
    token: &str,
    _keyword: &str,
    full: f32,
    channels: &[(&str, f32)],
) -> Option<f32> {
    signed(&channel_text(token, channels), full)
}

fn relative_hue(
    token: &str,
    _keyword: &str,
    _source: f32,
    channels: &[(&str, f32)],
) -> Option<f32> {
    hue(&channel_text(token, channels))
}

fn relative_fraction(
    token: &str,
    _keyword: &str,
    _source: f32,
    channels: &[(&str, f32)],
) -> Option<f32> {
    let value = percent(&channel_text(token, channels))?;
    Some(value.clamp(0.0, 1.0))
}

fn hwb_rgb(hue: f32, whiteness: f32, blackness: f32) -> (f32, f32, f32) {
    let (mut red, mut green, mut blue) = hsl_rgb(hue, 1.0, 0.5);
    let sum = whiteness + blackness;
    if sum >= 1.0 {
        let gray = whiteness / sum.max(f32::EPSILON);
        (red, green, blue) = (gray, gray, gray);
    } else {
        let factor = 1.0 - sum;
        red = red * factor + whiteness;
        green = green * factor + whiteness;
        blue = blue * factor + whiteness;
    }
    (red, green, blue)
}

/// `contrast-color(#000)` picks the ink that reads on a colour. The older
/// `contrast-color(#000 vs #fff, red)` form picks the candidate that reads
/// best against the first colour.
fn parse_contrast_color(inside: &str, light: bool, depth: usize) -> Option<Rgba> {
    let (background, candidates) = match split_at_keyword(inside, " vs ") {
        Some((background, candidates)) => (background, Some(candidates)),
        None => (inside, None),
    };
    let source = parse_depth(background.trim(), light, depth + 1)?;
    let Some(candidates) = candidates else {
        return Some(contrast_ink(source));
    };
    // The candidates are read in order, and the first one that reads as well
    // as the ones before it keeps the place.
    let mut best: Option<(Rgba, f32)> = None;
    for candidate in split_top(candidates, ',') {
        let candidate = parse_depth(candidate, light, depth + 1)?;
        let contrast = candidate.contrast(source);
        if best.is_none_or(|(_, best)| contrast > best) {
            best = Some((candidate, contrast));
        }
    }
    best.map(|(colour, _)| colour)
}

/// The place a keyword sits in a colour function's body, whatever its case.
fn split_at_keyword<'a>(text: &'a str, keyword: &str) -> Option<(&'a str, &'a str)> {
    let lower = text.to_ascii_lowercase();
    let at = lower.find(keyword)?;
    Some((&text[..at], &text[at + keyword.len()..]))
}

fn contrast_ink(source: Rgba) -> Rgba {
    let black = Rgba::rgb(0, 0, 0);
    let white = Rgba::rgb(255, 255, 255);
    if source.contrast(black) >= source.contrast(white) {
        black
    } else {
        white
    }
}

fn rgb_to_hsl(color: Rgba) -> (f32, f32, f32) {
    let red = f32::from(color.r) / 255.0;
    let green = f32::from(color.g) / 255.0;
    let blue = f32::from(color.b) / 255.0;
    let maximum = red.max(green).max(blue);
    let minimum = red.min(green).min(blue);
    let lightness = (maximum + minimum) / 2.0;
    let delta = maximum - minimum;
    let saturation = if delta <= f32::EPSILON {
        0.0
    } else {
        delta / (1.0 - (2.0 * lightness - 1.0).abs()).max(f32::EPSILON)
    };
    let hue = if delta <= f32::EPSILON {
        0.0
    } else if maximum == red {
        6e+01 * ((green - blue) / delta).rem_euclid(6.0)
    } else if maximum == green {
        6e+01 * ((blue - red) / delta + 2.0)
    } else {
        6e+01 * ((red - green) / delta + 4.0)
    };
    (hue.rem_euclid(3.6e+02), saturation, lightness)
}

fn rgb_to_hwb(color: Rgba) -> (f32, f32, f32) {
    let (hue, _, _) = rgb_to_hsl(color);
    let red = f32::from(color.r) / 255.0;
    let green = f32::from(color.g) / 255.0;
    let blue = f32::from(color.b) / 255.0;
    let white = red.min(green).min(blue);
    let black = 1.0 - red.max(green).max(blue);
    (hue, white, black)
}

fn rgb_to_oklab(color: Rgba) -> (f32, f32, f32) {
    let [red, green, blue] = linear_rgb(color);
    let l = 0.412_221_47 * red + 0.536_332_54 * green + 0.051_445_99 * blue;
    let m = 0.211_903_5 * red + 0.680_699_5 * green + 0.107_396_96 * blue;
    let s = 0.088_302_46 * red + 0.281_718_84 * green + 0.629_978_7 * blue;
    let l = l.cbrt();
    let m = m.cbrt();
    let s = s.cbrt();
    (
        0.210_454_26 * l + 0.793_617_8 * m - 0.004_072_047 * s,
        1.977_998_5 * l - 2.428_592_2 * m + 0.450_593_7 * s,
        0.025_904_037 * l + 0.782_771_77 * m - 0.808_675_77 * s,
    )
}

fn rgb_to_lab(color: Rgba) -> (f32, f32, f32) {
    let [red, green, blue] = linear_rgb(color);
    let x = 0.412_456_4 * red + 0.357_576_1 * green + 0.180_437_5 * blue;
    let y = 0.212_672_9 * red + 0.715_152_2 * green + 0.072_175 * blue;
    let z = 0.019_333_9 * red + 0.119_192 * green + 0.950_304_1 * blue;
    let f = |value: f32| {
        if value > 0.008_856 {
            value.cbrt()
        } else {
            (903.3 * value + 16.0) / 116.0
        }
    };
    let fx = f(x / 0.950_47);
    let fy = f(y);
    let fz = f(z / 1.088_83);
    (116.0 * fy - 16.0, 5e+02 * (fx - fy), 2e+02 * (fy - fz))
}

/// The colour's OKLCh channels: its lightness, its chroma and its hue in
/// degrees, as CSS's `oklch()` writes them. A colour with no chroma has no
/// hue, and its hue is 0.
pub fn to_oklch(colour: Rgba) -> (f32, f32, f32) {
    let (lightness, a, b) = rgb_to_oklab(colour);
    let chroma = (a * a + b * b).sqrt();
    let hue = if chroma <= MIN_OK_CHROMA {
        0.0
    } else {
        b.atan2(a).to_degrees().rem_euclid(3.6e+02)
    };
    (lightness, chroma, hue)
}

/// The colour's CIE LCh channels: its lightness, its chroma and its hue in
/// degrees, as CSS's `lch()` writes them. A colour with no chroma has no
/// hue, and its hue is 0.
pub fn to_lch(colour: Rgba) -> (f32, f32, f32) {
    let (lightness, a, b) = rgb_to_lab(colour);
    let chroma = (a * a + b * b).sqrt();
    let hue = if chroma <= MIN_LAB_CHROMA {
        0.0
    } else {
        b.atan2(a).to_degrees().rem_euclid(3.6e+02)
    };
    (lightness, chroma, hue)
}

/// The colour's CIE Lab channels: its lightness and its a and b axes, as
/// CSS's `lab()` writes them.
pub fn to_lab(colour: Rgba) -> (f32, f32, f32) {
    rgb_to_lab(colour)
}

fn linear_rgb(color: Rgba) -> [f32; 3] {
    [
        srgb_decode(f32::from(color.r) / 255.0),
        srgb_decode(f32::from(color.g) / 255.0),
        srgb_decode(f32::from(color.b) / 255.0),
    ]
}

fn alpha_value(alpha: Option<&str>, comma_alpha: Option<&str>) -> Option<u8> {
    if let Some(alpha) = alpha {
        return alpha_component(alpha);
    }
    comma_alpha.map(alpha_component).unwrap_or(Some(255))
}

/// The 8-bit channel of a value from 0 to 1, as the window paints it. A
/// value that is not a number cannot be painted, and the colour is lost.
fn channel(value: f32) -> Option<u8> {
    if !value.is_finite() {
        return None;
    }
    Some((value.clamp(0.0, 1.0) * 255.0).round() as u8)
}

/// The colour of three linear-light channels, as the window paints it.
fn srgb_bytes(linear: [f32; 3], alpha: u8) -> Option<Rgba> {
    Some(Rgba::rgba(
        channel(srgb_encode(linear[0])?)?,
        channel(srgb_encode(linear[1])?)?,
        channel(srgb_encode(linear[2])?)?,
        alpha,
    ))
}

/// The colour of three sRGB channels, as the window paints it.
fn hsl_bytes(channels: [f32; 3], alpha: u8) -> Option<Rgba> {
    Some(Rgba::rgba(
        channel(channels[0])?,
        channel(channels[1])?,
        channel(channels[2])?,
        alpha,
    ))
}

fn percent(text: &str) -> Option<f32> {
    let value = numeric(text)?;
    if value.angle {
        return None;
    }
    Some(value.value / 1e+02)
}

/// A saturation, a lightness, a whiteness or a blackness: a percentage, or a
/// number from 0 to 1 in the space-separated syntax. The legacy comma
/// syntax wants a percentage, as CSS does.
fn portion(text: &str, legacy: bool) -> Option<f32> {
    let value = numeric(text)?;
    if value.angle {
        return None;
    }
    if value.percent {
        return Some((value.value / 1e+02).clamp(0.0, 1.0));
    }
    if legacy {
        return None;
    }
    Some(value.value.clamp(0.0, 1.0))
}

fn number_or_percent(text: &str, full: f32) -> Option<f32> {
    let value = numeric(text)?;
    if value.angle {
        return None;
    }
    Some(if value.percent {
        value.value / 1e+02 * full
    } else {
        value.value
    })
}

fn signed(text: &str, full: f32) -> Option<f32> {
    number_or_percent(text, full)
}

/// A hue in degrees, radians, grades or turns.
fn hue(text: &str) -> Option<f32> {
    let value = numeric(text)?;
    if value.percent {
        return None;
    }
    Some(value.value.rem_euclid(3.6e+02))
}

fn hsl_rgb(hue: f32, saturation: f32, lightness: f32) -> (f32, f32, f32) {
    let chroma = (1.0 - (2.0 * lightness - 1.0).abs()) * saturation;
    let sector = hue / 6e+01;
    let x = chroma * (1.0 - (sector % 2.0 - 1.0).abs());
    let (r, g, b) = match sector as u32 {
        0 => (chroma, x, 0.0),
        1 => (x, chroma, 0.0),
        2 => (0.0, chroma, x),
        3 => (0.0, x, chroma),
        4 => (x, 0.0, chroma),
        _ => (chroma, 0.0, x),
    };
    let m = lightness - chroma / 2.0;
    (r + m, g + m, b + m)
}

/// `in srgb, #111 30%, #fff` with any number of colours and percentages, as
/// CSS mixes them. The common sRGB form is exact. The other spaces are mixed
/// in their own channels, and a hue travels along the shorter arc, as CSS
/// interpolates it.
fn parse_color_mix(inside: &str, light: bool, depth: usize) -> Option<Rgba> {
    let args = split_top(inside, ',');
    if args.len() < 3 {
        return None;
    }
    let space = args[0].trim().to_ascii_lowercase();
    if !space.starts_with("in ") {
        return None;
    }
    let mixing = Mix::of(&space)?;
    let mut colours: Vec<(Rgba, Option<f32>)> = Vec::new();
    for arg in &args[1..] {
        let (colour_text, percentage) = take_percentage(arg);
        let colour = parse_depth(colour_text, light, depth)?;
        colours.push((colour, percentage));
    }
    if colours.len() < 2 {
        return None;
    }
    let specified: f32 = colours.iter().filter_map(|(_, share)| *share).sum();
    let unspecified = colours.iter().filter(|(_, share)| share.is_none()).count();
    let weights: Vec<f32> = if unspecified == 0 {
        if specified <= f32::EPSILON {
            return Some(Rgba::TRANSPARENT);
        }
        colours
            .iter()
            .map(|(_, share)| share.unwrap_or(0.0) / specified)
            .collect()
    } else if specified >= 1.0 {
        let share_of_unspecified = 0.0;
        colours
            .iter()
            .map(|(_, share)| match share {
                Some(share) => share / specified,
                None => share_of_unspecified,
            })
            .collect()
    } else {
        let each = (1.0 - specified.max(0.0)) / unspecified as f32;
        colours
            .iter()
            .map(|(_, share)| share.unwrap_or(each))
            .collect()
    };
    let total: f32 = weights.iter().sum();
    if total <= f32::EPSILON {
        return Some(Rgba::TRANSPARENT);
    }
    // With no unspecified colour, percentages below 100% scale the result's
    // alpha, as CSS color-mix does; above 100% they are normalised back.
    let alpha_multiplier = if unspecified == 0 {
        specified.min(1.0)
    } else {
        1.0
    };
    let mut alpha = 0.0f32;
    let mut channels = [0.0f32; 3];
    let hue_index = mixing.space.hue_index();
    // The hue travels from the first colour's hue along the arc the method
    // names, as CSS Color 4's hue interpolation does.
    let mut anchor: Option<f32> = None;
    let mut travel = 0.0f32;
    for ((colour, _), weight) in colours.iter().zip(&weights) {
        let share = f32::from(colour.a) / 255.0 * weight;
        alpha += share;
        let coordinates = mixing.space.coordinates(*colour);
        for (index, value) in coordinates.iter().enumerate() {
            if Some(index) == hue_index {
                match anchor {
                    None => anchor = Some(*value),
                    Some(first) => travel += hue_arc(first, *value, mixing.hue) * share,
                }
            } else {
                channels[index] += value * share;
            }
        }
    }
    if alpha <= f32::EPSILON {
        return Some(Rgba::TRANSPARENT);
    }
    for (index, channel) in channels.iter_mut().enumerate() {
        if Some(index) != hue_index {
            *channel /= alpha;
        }
    }
    if let Some(index) = hue_index {
        let first = anchor.unwrap_or(0.0);
        channels[index] = (first + travel / alpha).rem_euclid(3.6e+02);
    }
    let mut mixed = mixing.space.rgba(channels, 255)?;
    mixed.a = (alpha * alpha_multiplier * 255.0).round().clamp(0.0, 255.0) as u8;
    Some(mixed)
}

/// The interpolation a `color-mix()` names: the space the colours are mixed
/// in, and the way a hue travels between them.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Mix {
    space: MixSpace,
    hue: HueMethod,
}

impl Mix {
    /// The interpolation a `color-mix()` names, when it names one the reader
    /// knows.
    fn of(text: &str) -> Option<Self> {
        let rest = text.strip_prefix("in ")?.trim();
        let (name, hue) = match rest.split_once(char::is_whitespace) {
            Some((name, rest)) => {
                let (how, hue) = rest.trim().split_once(char::is_whitespace)?;
                if !hue.trim().eq_ignore_ascii_case("hue") {
                    return None;
                }
                (name.trim(), HueMethod::of(how.trim())?)
            }
            None => (rest, HueMethod::Shorter),
        };
        Some(Self {
            space: MixSpace::of(name),
            hue,
        })
    }
}

/// The way a hue travels between two colours, as CSS Color 5 names it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum HueMethod {
    Shorter,
    Longer,
    Increasing,
    Decreasing,
}

impl HueMethod {
    /// The method a `color-mix()` names, when it names one CSS has.
    fn of(text: &str) -> Option<Self> {
        match text {
            "shorter" => Some(Self::Shorter),
            "longer" => Some(Self::Longer),
            "increasing" => Some(Self::Increasing),
            "decreasing" => Some(Self::Decreasing),
            _ => None,
        }
    }
}

/// The angle from the first hue to the second, along the arc the method
/// names, as CSS Color 4's hue interpolation works it out. A difference of
/// exactly 180 degrees keeps the rising direction, as the shorter arc does.
fn hue_arc(first: f32, second: f32, method: HueMethod) -> f32 {
    let mut delta = second - first;
    match method {
        HueMethod::Shorter => {
            if delta > 1.8e+02 {
                delta -= 3.6e+02;
            } else if delta < -1.8e+02 {
                delta += 3.6e+02;
            }
        }
        HueMethod::Longer => {
            if delta > 0.0 && delta < 1.8e+02 {
                delta -= 3.6e+02;
            } else if delta > -1.8e+02 && delta <= 0.0 {
                delta += 3.6e+02;
            }
        }
        HueMethod::Increasing => {
            if delta < 0.0 {
                delta += 3.6e+02;
            }
        }
        HueMethod::Decreasing => {
            if delta > 0.0 {
                delta -= 3.6e+02;
            }
        }
    }
    delta
}

/// The space a `color-mix()` mixes in. The spaces CSS Color 4 defines are
/// mixed in their own channels; a space the reader does not know is mixed
/// in sRGB, as it has always done, which keeps the colours a theme wrote
/// usable.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum MixSpace {
    /// One of the predefined colour spaces.
    Predefined(ColorSpace),
    /// HSL, in sRGB.
    Hsl,
    /// HWB, in sRGB.
    Hwb,
    /// CIE Lab.
    Lab,
    /// CIE LCh.
    Lch,
    /// OKLab.
    Oklab,
    /// OKLCh.
    Oklch,
}

impl MixSpace {
    /// The space a `color-mix()` names.
    fn of(name: &str) -> Self {
        match name {
            "srgb-linear" => Self::Predefined(ColorSpace::SrgbLinear),
            "display-p3" => Self::Predefined(ColorSpace::DisplayP3),
            "display-p3-linear" => Self::Predefined(ColorSpace::DisplayP3Linear),
            "a98-rgb" => Self::Predefined(ColorSpace::A98Rgb),
            "prophoto-rgb" => Self::Predefined(ColorSpace::ProPhotoRgb),
            "rec2020" => Self::Predefined(ColorSpace::Rec2020),
            "xyz" | "xyz-d65" => Self::Predefined(ColorSpace::XyzD65),
            "xyz-d50" => Self::Predefined(ColorSpace::XyzD50),
            "hsl" => Self::Hsl,
            "hwb" => Self::Hwb,
            "lab" => Self::Lab,
            "lch" => Self::Lch,
            "oklab" => Self::Oklab,
            "oklch" => Self::Oklch,
            _ => Self::Predefined(ColorSpace::Srgb),
        }
    }

    /// The channel holding a circular hue, for the spaces that have one.
    fn hue_index(self) -> Option<usize> {
        match self {
            Self::Hsl | Self::Hwb => Some(0),
            Self::Oklch | Self::Lch => Some(2),
            _ => None,
        }
    }

    /// Whether the space's channels are sRGB's own, 0..1 as the window
    /// writes them. A mix in it reads the bytes and paints them back without
    /// a decode and an encode, which round a half down: `color-mix(in srgb,
    /// #000, #fff)` is the common case and must come out at 128, not 127.
    fn is_srgb(self) -> bool {
        matches!(self, Self::Predefined(ColorSpace::Srgb))
    }

    /// The colour's channels in the space, as the mix reads them.
    fn coordinates(self, color: Rgba) -> [f32; 3] {
        match self {
            _ if self.is_srgb() => [
                f32::from(color.r) / 255.0,
                f32::from(color.g) / 255.0,
                f32::from(color.b) / 255.0,
            ],
            Self::Predefined(space) => space
                .channels_from_linear(linear_rgb(color))
                .unwrap_or([0.0; 3]),
            Self::Hsl => {
                let (hue, saturation, lightness) = rgb_to_hsl(color);
                [hue, saturation, lightness]
            }
            Self::Hwb => {
                let (hue, white, black) = rgb_to_hwb(color);
                [hue, white, black]
            }
            Self::Oklab => {
                let (lightness, a, b) = rgb_to_oklab(color);
                [lightness, a, b]
            }
            Self::Oklch => {
                let (lightness, chroma, hue) = to_oklch(color);
                [lightness, chroma, hue]
            }
            Self::Lab => {
                let (lightness, a, b) = rgb_to_lab(color);
                [lightness, a, b]
            }
            Self::Lch => {
                let (lightness, chroma, hue) = to_lch(color);
                [lightness, chroma, hue]
            }
        }
    }

    /// The colour the mixed channels make, as the window paints it.
    fn rgba(self, coordinates: [f32; 3], alpha: u8) -> Option<Rgba> {
        match self {
            _ if self.is_srgb() => hsl_bytes(coordinates, alpha),
            Self::Predefined(space) => srgb_bytes(space.to_linear(coordinates)?, alpha),
            Self::Hsl => {
                let (red, green, blue) = hsl_rgb(coordinates[0], coordinates[1], coordinates[2]);
                hsl_bytes([red, green, blue], alpha)
            }
            Self::Hwb => {
                let (red, green, blue) = hwb_rgb(coordinates[0], coordinates[1], coordinates[2]);
                hsl_bytes([red, green, blue], alpha)
            }
            Self::Oklab => oklab_rgba(coordinates[0], coordinates[1], coordinates[2], alpha),
            Self::Oklch => {
                let hue = coordinates[2].to_radians();
                oklab_rgba(
                    coordinates[0],
                    coordinates[1] * hue.cos(),
                    coordinates[1] * hue.sin(),
                    alpha,
                )
            }
            Self::Lab => lab_rgba(coordinates[0], coordinates[1], coordinates[2], alpha),
            Self::Lch => {
                let hue = coordinates[2].to_radians();
                lab_rgba(
                    coordinates[0],
                    coordinates[1] * hue.cos(),
                    coordinates[1] * hue.sin(),
                    alpha,
                )
            }
        }
    }
}

/// A colour space CSS Color 4 defines, and the way its channels and sRGB's
/// meet.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ColorSpace {
    Srgb,
    SrgbLinear,
    DisplayP3,
    DisplayP3Linear,
    A98Rgb,
    ProPhotoRgb,
    Rec2020,
    XyzD50,
    XyzD65,
}

/// The matrix from linear sRGB to XYZ (D65), from the CSS Color 4 sample
/// code.
const SRGB_TO_XYZ: [[f32; 3]; 3] = [
    [
        506_752.0 / 1_228_815.0,
        87_881.0 / 245_763.0,
        12_673.0 / 70_218.0,
    ],
    [
        87_098.0 / 409_605.0,
        175_762.0 / 245_763.0,
        12_673.0 / 175_545.0,
    ],
    [
        7_918.0 / 409_605.0,
        87_881.0 / 737_289.0,
        1_001_167.0 / 1_053_270.0,
    ],
];

/// The matrix from XYZ (D65) to linear sRGB, from the CSS Color 4 sample
/// code.
const XYZ_TO_SRGB: [[f32; 3]; 3] = [
    [12_831.0 / 3_959.0, -329.0 / 214.0, -1_974.0 / 3_959.0],
    [
        -851_781.0 / 878_810.0,
        1_648_619.0 / 878_810.0,
        36_519.0 / 878_810.0,
    ],
    [705.0 / 12_673.0, -2_585.0 / 12_673.0, 705.0 / 667.0],
];

/// The matrix from linear Display P3 to XYZ (D65), from the CSS Color 4
/// sample code.
const P3_TO_XYZ: [[f32; 3]; 3] = [
    [
        608_311.0 / 1_250_200.0,
        189_793.0 / 714_400.0,
        198_249.0 / 1_000_160.0,
    ],
    [
        35_783.0 / 156_275.0,
        247_089.0 / 357_200.0,
        198_249.0 / 2_500_400.0,
    ],
    [0.0, 32_229.0 / 714_400.0, 5_220_557.0 / 5_000_800.0],
];

/// The matrix from XYZ (D65) to linear Display P3, from the CSS Color 4
/// sample code.
const XYZ_TO_P3: [[f32; 3]; 3] = [
    [
        446_124.0 / 178_915.0,
        -333_277.0 / 357_830.0,
        -72_051.0 / 178_915.0,
    ],
    [-14_852.0 / 17_905.0, 63_121.0 / 35_810.0, 423.0 / 17_905.0],
    [
        11_844.0 / 330_415.0,
        -50_337.0 / 660_830.0,
        316_169.0 / 330_415.0,
    ],
];

/// The matrix from linear A98 RGB to XYZ (D65), from the CSS Color 4 sample
/// code.
const A98_TO_XYZ: [[f32; 3]; 3] = [
    [
        573_536.0 / 994_567.0,
        263_643.0 / 1_420_810.0,
        187_206.0 / 994_567.0,
    ],
    [
        591_459.0 / 1_989_134.0,
        6_239_551.0 / 9_945_670.0,
        374_412.0 / 4_972_835.0,
    ],
    [
        53_769.0 / 1_989_134.0,
        351_524.0 / 4_972_835.0,
        4_929_758.0 / 4_972_835.0,
    ],
];

/// The matrix from XYZ (D65) to linear A98 RGB, from the CSS Color 4 sample
/// code.
const XYZ_TO_A98: [[f32; 3]; 3] = [
    [
        1_829_569.0 / 896_150.0,
        -506_331.0 / 896_150.0,
        -308_931.0 / 896_150.0,
    ],
    [
        -851_781.0 / 878_810.0,
        1_648_619.0 / 878_810.0,
        36_519.0 / 878_810.0,
    ],
    [
        16_779.0 / 1_248_040.0,
        -147_721.0 / 1_248_040.0,
        1_266_979.0 / 1_248_040.0,
    ],
];

/// The matrix from linear ProPhoto RGB to XYZ (D50), from the CSS Color 4
/// sample code.
const PROPHOTO_TO_XYZ: [[f32; 3]; 3] = [
    [0.7977666, 0.1351813, 0.031347734],
    [0.28807483, 0.7118352, 8.993694e-05],
    [0.0, 0.0, 0.8251046],
];

/// The matrix from XYZ (D50) to linear ProPhoto RGB, from the CSS Color 4
/// sample code.
const XYZ_TO_PROPHOTO: [[f32; 3]; 3] = [
    [1.3457869, -0.25557209, -0.051101865],
    [-0.5446307, 1.5082477, 0.020527447],
    [0.0, 0.0, 1.2119676],
];

/// The matrix from linear Rec. 2020 to XYZ (D65), from the CSS Color 4
/// sample code.
const REC2020_TO_XYZ: [[f32; 3]; 3] = [
    [
        63_426_534.0 / 99_577_255.0,
        20_160_776.0 / 139_408_157.0,
        47_086_771.0 / 278_816_314.0,
    ],
    [
        26_158_966.0 / 99_577_255.0,
        472_592_308.0 / 697_040_785.0,
        8_267_143.0 / 139_408_157.0,
    ],
    [
        0.0,
        19_567_812.0 / 697_040_785.0,
        295_819_943.0 / 278_816_314.0,
    ],
];

/// The matrix from XYZ (D65) to linear Rec. 2020, from the CSS Color 4
/// sample code.
const XYZ_TO_REC2020: [[f32; 3]; 3] = [
    [
        30_757_411.0 / 17_917_100.0,
        -6_372_589.0 / 17_917_100.0,
        -4_539_589.0 / 17_917_100.0,
    ],
    [
        -19_765_991.0 / 29_648_200.0,
        47_925_759.0 / 29_648_200.0,
        467_509.0 / 29_648_200.0,
    ],
    [
        792_561.0 / 44_930_125.0,
        -1_921_689.0 / 44_930_125.0,
        42_328_811.0 / 44_930_125.0,
    ],
];

/// The Bradford adaptation from D65 to D50, from the CSS Color 4 sample
/// code.
const D65_TO_D50: [[f32; 3]; 3] = [
    [1.0479298, 0.02294687, -0.050192266],
    [0.02962781, 0.9904344, -0.017073799],
    [-0.00924304, 0.015055192, 0.75187428],
];

/// The Bradford adaptation from D50 to D65, from the CSS Color 4 sample
/// code.
const D50_TO_D65: [[f32; 3]; 3] = [
    [0.9554734, -0.023098455, 0.063259243],
    [-0.02836971, 1.0099954, 0.021041441],
    [0.012314015, -0.020507649, 1.3303659],
];

impl ColorSpace {
    /// The space a `color()` names, when CSS defines it. A space CSS does
    /// not define is not a colour.
    fn of(name: &str) -> Option<Self> {
        match name {
            "srgb" => Some(Self::Srgb),
            "srgb-linear" => Some(Self::SrgbLinear),
            "display-p3" => Some(Self::DisplayP3),
            "display-p3-linear" => Some(Self::DisplayP3Linear),
            "a98-rgb" => Some(Self::A98Rgb),
            "prophoto-rgb" => Some(Self::ProPhotoRgb),
            "rec2020" => Some(Self::Rec2020),
            "xyz" | "xyz-d65" => Some(Self::XyzD65),
            "xyz-d50" => Some(Self::XyzD50),
            _ => None,
        }
    }

    /// The names CSS gives the space's three channels in a relative colour:
    /// r, g and b for an additive space, x, y and z for XYZ.
    fn channel_names(self) -> [&'static str; 3] {
        match self {
            Self::XyzD50 | Self::XyzD65 => ["x", "y", "z"],
            _ => ["r", "g", "b"],
        }
    }

    /// The colour's XYZ (D65), from the space's own channels.
    fn to_xyz(self, channels: [f32; 3]) -> [f32; 3] {
        match self {
            Self::Srgb => matrix(SRGB_TO_XYZ, channels.map(srgb_decode)),
            Self::SrgbLinear => matrix(SRGB_TO_XYZ, channels),
            Self::DisplayP3 => matrix(P3_TO_XYZ, channels.map(srgb_decode)),
            Self::DisplayP3Linear => matrix(P3_TO_XYZ, channels),
            Self::A98Rgb => matrix(A98_TO_XYZ, channels.map(a98_decode)),
            Self::ProPhotoRgb => matrix(
                D50_TO_D65,
                matrix(PROPHOTO_TO_XYZ, channels.map(prophoto_decode)),
            ),
            Self::Rec2020 => matrix(REC2020_TO_XYZ, channels.map(rec2020_decode)),
            Self::XyzD50 => matrix(D50_TO_D65, channels),
            Self::XyzD65 => channels,
        }
    }

    /// The space's own channels, from the colour's XYZ (D65).
    fn channels_from_xyz(self, xyz: [f32; 3]) -> Option<[f32; 3]> {
        match self {
            Self::Srgb => transferred(matrix(XYZ_TO_SRGB, xyz), srgb_encode),
            Self::SrgbLinear => Some(matrix(XYZ_TO_SRGB, xyz)),
            Self::DisplayP3 => transferred(matrix(XYZ_TO_P3, xyz), srgb_encode),
            Self::DisplayP3Linear => Some(matrix(XYZ_TO_P3, xyz)),
            Self::A98Rgb => transferred(matrix(XYZ_TO_A98, xyz), a98_encode),
            Self::ProPhotoRgb => transferred(
                matrix(XYZ_TO_PROPHOTO, matrix(D65_TO_D50, xyz)),
                prophoto_encode,
            ),
            Self::Rec2020 => transferred(matrix(XYZ_TO_REC2020, xyz), rec2020_encode),
            Self::XyzD50 => Some(matrix(D65_TO_D50, xyz)),
            Self::XyzD65 => Some(xyz),
        }
    }

    /// The colour's linear-light sRGB channels, from the space's channels.
    ///
    /// sRGB is its own space, so it goes through no matrix: a round trip
    /// through XYZ loses the half a mix of two sRGB colours often lands on,
    /// and `color-mix(in srgb, #000, #fff)` would come out a channel low.
    fn to_linear(self, channels: [f32; 3]) -> Option<[f32; 3]> {
        match self {
            Self::Srgb => Some(channels.map(srgb_decode)),
            Self::SrgbLinear => Some(channels),
            _ => Some(matrix(XYZ_TO_SRGB, self.to_xyz(channels))),
        }
    }

    /// The space's channels, from the colour's linear-light sRGB channels.
    fn channels_from_linear(self, linear: [f32; 3]) -> Option<[f32; 3]> {
        match self {
            Self::Srgb => {
                let mut channels = [0.0; 3];
                for (channel, value) in channels.iter_mut().zip(linear) {
                    *channel = srgb_encode(value)?;
                }
                Some(channels)
            }
            Self::SrgbLinear => Some(linear),
            _ => self.channels_from_xyz(matrix(SRGB_TO_XYZ, linear)),
        }
    }
}

/// The product of a matrix and a colour's three channels.
fn matrix(m: [[f32; 3]; 3], v: [f32; 3]) -> [f32; 3] {
    [
        m[0][0] * v[0] + m[0][1] * v[1] + m[0][2] * v[2],
        m[1][0] * v[0] + m[1][1] * v[1] + m[1][2] * v[2],
        m[2][0] * v[0] + m[2][1] * v[1] + m[2][2] * v[2],
    ]
}

/// The three channels of a colour, each put through a transfer function.
fn transferred(channels: [f32; 3], transfer: fn(f32) -> Option<f32>) -> Option<[f32; 3]> {
    let mut out = [0.0f32; 3];
    for (index, value) in channels.iter().enumerate() {
        out[index] = transfer(*value)?;
    }
    Some(out)
}

/// The linear-light value of an A98 RGB channel: a gamma of 563/256.
fn a98_decode(value: f32) -> f32 {
    let sign = if value < 0.0 { -1.0 } else { 1.0 };
    sign * value.abs().powf(563.0 / 256.0)
}

/// The A98 RGB channel of a linear-light value.
fn a98_encode(value: f32) -> Option<f32> {
    if !value.is_finite() {
        return None;
    }
    let sign = if value < 0.0 { -1.0 } else { 1.0 };
    Some(sign * value.abs().powf(256.0 / 563.0))
}

/// The linear-light value of a ProPhoto RGB channel: a gamma of 1.8 with a
/// small linear part near black.
fn prophoto_decode(value: f32) -> f32 {
    if value.abs() <= 16.0 / 512.0 {
        return value / 16.0;
    }
    let sign = if value < 0.0 { -1.0 } else { 1.0 };
    sign * value.abs().powf(1.8)
}

/// The ProPhoto RGB channel of a linear-light value.
fn prophoto_encode(value: f32) -> Option<f32> {
    if !value.is_finite() {
        return None;
    }
    if value.abs() < 1.0 / 512.0 {
        return Some(16.0 * value);
    }
    let sign = if value < 0.0 { -1.0 } else { 1.0 };
    Some(sign * value.abs().powf(1.0 / 1.8))
}

/// The linear-light value of a Rec. 2020 channel: a gamma of 2.4.
fn rec2020_decode(value: f32) -> f32 {
    let sign = if value < 0.0 { -1.0 } else { 1.0 };
    sign * value.abs().powf(2.4)
}

/// The Rec. 2020 channel of a linear-light value.
fn rec2020_encode(value: f32) -> Option<f32> {
    if !value.is_finite() {
        return None;
    }
    let sign = if value < 0.0 { -1.0 } else { 1.0 };
    Some(sign * value.abs().powf(1.0 / 2.4))
}

/// Splits a trailing `30%` from a colour, as `color-mix` writes it.
fn take_percentage(text: &str) -> (&str, Option<f32>) {
    let text = text.trim();
    let Some(space) = text.rfind(char::is_whitespace) else {
        return (text, None);
    };
    let (colour, percent) = text.split_at(space);
    let percent = percent.trim();
    match percent.strip_suffix('%') {
        Some(value) => match value.trim().parse::<f32>() {
            Ok(value) => (colour.trim(), Some((value / 1e+02).clamp(0.0, 1.0))),
            Err(_) => (text, None),
        },
        None => (text, None),
    }
}

/// CSS's named colours, the ones Discord themes use by name.
fn named(name: &str) -> Option<(u8, u8, u8)> {
    let value = match name {
        "aliceblue" => 0xf0f8ff,
        "antiquewhite" => 0xfaebd7,
        "aqua" | "cyan" => 0x00ffff,
        "aquamarine" => 0x7fffd4,
        "azure" => 0xf0ffff,
        "beige" => 0xf5f5dc,
        "bisque" => 0xffe4c4,
        "blanchedalmond" => 0xffebcd,
        "blue" => 0x0000ff,
        "blueviolet" => 0x8a2be2,
        "brown" => 0xa52a2a,
        "burlywood" => 0xdeb887,
        "cadetblue" => 0x5f9ea0,
        "chartreuse" => 0x7fff00,
        "chocolate" => 0xd2691e,
        "coral" => 0xff7f50,
        "cornflowerblue" => 0x6495ed,
        "cornsilk" => 0xfff8dc,
        "crimson" => 0xdc143c,
        "darkblue" => 0x00008b,
        "darkcyan" => 0x008b8b,
        "darkgoldenrod" => 0xb8860b,
        "darkgray" | "darkgrey" => 0xa9a9a9,
        "darkgreen" => 0x006400,
        "darkkhaki" => 0xbdb76b,
        "darkmagenta" => 0x8b008b,
        "darkolivegreen" => 0x556b2f,
        "darkorange" => 0xff8c00,
        "darkorchid" => 0x9932cc,
        "darkred" => 0x8b0000,
        "darksalmon" => 0xe9967a,
        "darkseagreen" => 0x8fbc8f,
        "darkslateblue" => 0x483d8f,
        "darkslategray" | "darkslategrey" => 0x2f4f4f,
        "darkturquoise" => 0x00ced1,
        "darkviolet" => 0x9400d3,
        "deeppink" => 0xff1493,
        "deepskyblue" => 0x00bfff,
        "dimgray" | "dimgrey" => 0x696969,
        "dodgerblue" => 0x1e90ff,
        "firebrick" => 0xb22222,
        "floralwhite" => 0xfffaf0,
        "forestgreen" => 0x228b22,
        "fuchsia" | "magenta" => 0xff00ff,
        "gainsboro" => 0xdcdcdc,
        "ghostwhite" => 0xf8f8ff,
        "gold" => 0xffd700,
        "goldenrod" => 0xdaa520,
        "gray" | "grey" => 0x808080,
        "green" => 0x008000,
        "greenyellow" => 0xadff2f,
        "honeydew" => 0xf0fff0,
        "hotpink" => 0xff69b4,
        "indianred" => 0xcd5c5c,
        "indigo" => 0x4b0082,
        "ivory" => 0xfffff0,
        "khaki" => 0xf0e68c,
        "lavender" => 0xe6e6fa,
        "lavenderblush" => 0xfff0f5,
        "lawngreen" => 0x7cfc00,
        "lemonchiffon" => 0xfffacd,
        "lightblue" => 0xadd8e6,
        "lightcoral" => 0xf08080,
        "lightcyan" => 0xe0ffff,
        "lightgoldenrodyellow" => 0xfafad2,
        "lightgray" | "lightgrey" => 0xd3d3d3,
        "lightgreen" => 0x90ee90,
        "lightpink" => 0xffb6c1,
        "lightsalmon" => 0xffa07a,
        "lightseagreen" => 0x20b2aa,
        "lightskyblue" => 0x87cefa,
        "lightslategray" | "lightslategrey" => 0x778899,
        "lightsteelblue" => 0xb0c4de,
        "lightyellow" => 0xffffe0,
        "lime" => 0x00ff00,
        "limegreen" => 0x32cd32,
        "linen" => 0xfaf0e6,
        "maroon" => 0x800000,
        "mediumaquamarine" => 0x66cdaa,
        "mediumblue" => 0x0000cd,
        "mediumorchid" => 0xba55d3,
        "mediumpurple" => 0x9370db,
        "mediumseagreen" => 0x3cb371,
        "mediumslateblue" => 0x7b68ee,
        "mediumspringgreen" => 0x00fa9a,
        "mediumturquoise" => 0x48d1cc,
        "mediumvioletred" => 0xc71585,
        "midnightblue" => 0x191970,
        "mintcream" => 0xf5fffa,
        "mistyrose" => 0xffe1e1,
        "moccasin" => 0xffe4b5,
        "navajowhite" => 0xffdead,
        "navy" => 0x000080,
        "oldlace" => 0xfdf5e6,
        "olive" => 0x808000,
        "olivedrab" => 0x6b8e23,
        "orange" => 0xffa500,
        "orangered" => 0xff4500,
        "orchid" => 0xda70d6,
        "palegoldenrod" => 0xeee8aa,
        "palegreen" => 0x98fb98,
        "paleturquoise" => 0xafeeee,
        "palevioletred" => 0xdb7093,
        "papayawhip" => 0xffefd5,
        "peachpuff" => 0xffdab9,
        "peru" => 0xcd853f,
        "pink" => 0xffc0cb,
        "plum" => 0xdda0dd,
        "powderblue" => 0xb0e0e6,
        "purple" => 0x800080,
        "rebeccapurple" => 0x663399,
        "red" => 0xff0000,
        "rosybrown" => 0xbc8f8f,
        "royalblue" => 0x4169e1,
        "saddlebrown" => 0x8b4513,
        "salmon" => 0xfa8072,
        "sandybrown" => 0xf4a460,
        "seagreen" => 0x2e8b57,
        "seashell" => 0xfff5ee,
        "sienna" => 0xa0522d,
        "silver" => 0xc0c0c0,
        "skyblue" => 0x87ceeb,
        "slateblue" => 0x6a5acd,
        "slategray" | "slategrey" => 0x708090,
        "snow" => 0xfffafa,
        "springgreen" => 0x00ff7f,
        "steelblue" => 0x4682b4,
        "tan" => 0xd2b48c,
        "teal" => 0x008080,
        "thistle" => 0xd8bfd8,
        "tomato" => 0xff6347,
        "turquoise" => 0x40e0d0,
        "violet" => 0xee82ee,
        "wheat" => 0xf5deb3,
        "whitesmoke" => 0xf5f5f5,
        "yellow" => 0xffff00,
        "yellowgreen" => 0x9acd32,
        _ => return None,
    };
    Some((
        ((value >> 16) & 0xff) as u8,
        ((value >> 8) & 0xff) as u8,
        (value & 0xff) as u8,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hex_colours_come_in_every_length() {
        assert_eq!(parse("#fff", true), Some(Rgba::rgb(255, 255, 255)));
        assert_eq!(parse("#e94221", true), Some(Rgba::rgb(0xe9, 0x42, 0x21)));
        assert_eq!(
            parse("#e9422129", true),
            Some(Rgba::rgba(0xe9, 0x42, 0x21, 0x29))
        );
        assert_eq!(parse("#ggg", true), None);
    }

    #[test]
    fn rgb_and_hsl_take_both_syntaxes() {
        assert_eq!(
            parse("rgb(233, 66, 33)", true),
            Some(Rgba::rgb(233, 66, 33))
        );
        assert_eq!(
            parse("rgba(233 66 33 / 50%)", true),
            Some(Rgba::rgba(233, 66, 33, 128))
        );
        assert_eq!(parse("hsl(0 0% 0%)", true), Some(Rgba::rgb(0, 0, 0)));
        assert_eq!(parse("hsl(0, 100%, 50%)", true), Some(Rgba::rgb(255, 0, 0)));
    }

    #[test]
    fn color_mix_mixes_in_srgb() {
        let mixed = parse("color-mix(in srgb, #e94221 80%, #fff3d6)", true).unwrap();
        assert_eq!(mixed, Rgba::rgb(0xed, 0x65, 0x45));
        let halfway = parse("color-mix(in srgb, white, black)", true).unwrap();
        assert_eq!(halfway, Rgba::rgb(128, 128, 128));
        let clear = parse("color-mix(in srgb, #e94221 34%, transparent)", true).unwrap();
        assert_eq!(clear, Rgba::rgba(0xe9, 0x42, 0x21, 87));
    }

    #[test]
    fn light_dark_picks_by_mode() {
        assert_eq!(
            parse("light-dark(#fff, #000)", true),
            Some(Rgba::rgb(255, 255, 255))
        );
        assert_eq!(
            parse("light-dark(#fff, #000)", false),
            Some(Rgba::rgb(0, 0, 0))
        );
    }

    #[test]
    fn lab_lch_and_colour_spaces_read_as_colours() {
        let lab = parse("lab(53.24 80.09 67.2)", true).unwrap();
        assert!(lab.r > 220 && lab.g < 80 && lab.b < 80, "{lab:?}");
        let lch = parse("lch(53.24 104.55 40)", true).unwrap();
        assert!(lch.r > 220 && lch.g < 90 && lch.b < 90, "{lch:?}");
        let srgb = parse("color(srgb 0.5 0.5 0.5)", true).unwrap();
        assert_eq!(srgb, Rgba::rgb(128, 128, 128));
        let p3 = parse("color(display-p3 1 0 0)", true).unwrap();
        assert!(p3.r > 240 && p3.g < 20 && p3.b < 20, "{p3:?}");
        assert_eq!(
            parse("color(srgb 1 1 1 / 50%)", true),
            Some(Rgba::rgba(255, 255, 255, 128))
        );
    }

    #[test]
    fn oklab_and_oklch_read_as_colours() {
        let lab = parse("oklab(0.6279 0.2249 0.1258)", true).unwrap();
        assert!(lab.r > 200 && lab.g < 100 && lab.b < 100, "{lab:?}");
        let lch = parse("oklch(0.6279 0.2577 29.23)", true).unwrap();
        assert!(lch.r > 200 && lch.g < 120, "{lch:?}");
        assert_eq!(parse("hwb(0 0% 0%)", true), Some(Rgba::rgb(255, 0, 0)));
    }

    #[test]
    fn color_mix_takes_more_than_two_colours() {
        let mixed = parse("color-mix(in srgb, #ff0000, #00ff00, #0000ff)", true).unwrap();
        assert_eq!(mixed, Rgba::rgb(85, 85, 85));
        let weighted = parse(
            "color-mix(in srgb, #ff0000 50%, #00ff00 25%, #0000ff)",
            true,
        )
        .unwrap();
        // The last colour takes the remaining 25%.
        assert_eq!(weighted, Rgba::rgb(128, 64, 64));
        let short = parse("color-mix(in srgb, #ff0000 30%, #0000ff 30%)", true).unwrap();
        assert_eq!(short, Rgba::rgba(128, 0, 128, 153));
    }

    #[test]
    fn mixing_takes_the_share_of_the_other_colour() {
        let mixed = Rgba::rgb(0, 0, 0).mix(Rgba::rgb(255, 255, 255), 0.25);
        assert_eq!(mixed, Rgba::rgb(64, 64, 64));
        let clear = Rgba::rgb(255, 0, 0).mix(Rgba::TRANSPARENT, 0.5);
        assert_eq!(clear, Rgba::rgba(255, 0, 0, 128));
    }

    #[test]
    fn an_over_puts_a_translucent_layer_on_its_ground() {
        let red = Rgba::rgba(255, 0, 0, 128);
        let black = Rgba::rgb(0, 0, 0);
        let mixed = red.over(black);
        assert_eq!((mixed.r, mixed.g, mixed.b), (128, 0, 0));
    }

    #[test]
    fn calc_settles_hsl_channels() {
        assert_eq!(
            parse("hsl(220, calc(1 * 8%), 23%)", false),
            Some(Rgba::rgb(54, 57, 63))
        );
        assert_eq!(
            parse("hsla(213, calc(1 * 13%), calc(75% - 0% + 0%), 1)", false),
            Some(Rgba::rgb(183, 190, 200))
        );
    }

    #[test]
    fn relative_colours_keep_or_replace_their_source_channels() {
        assert_eq!(
            parse("rgb(from #336699 r g b)", true),
            Some(Rgba::rgb(0x33, 0x66, 0x99))
        );
        assert_eq!(
            parse("rgb(from #336699 r g b / 50%)", true),
            Some(Rgba::rgba(0x33, 0x66, 0x99, 128))
        );
        assert_eq!(
            parse("hsl(from #ff0000 h s l)", true),
            Some(Rgba::rgb(255, 0, 0))
        );
        let round_trip = parse("oklch(from #336699 l c h)", true).unwrap();
        assert_eq!(
            (round_trip.r, round_trip.g, round_trip.b),
            (0x33, 0x66, 0x99)
        );
        let newer = parse("oklch(from #336699 0.8 c h)", true).unwrap();
        assert!(newer.r < 150 && newer.g > 150, "{newer:?}");
        assert_eq!(
            parse("hsl(from #dcdcde h calc(1 * s) l/alpha)", true),
            Some(Rgba::rgb(0xdc, 0xdc, 0xde))
        );
    }

    #[test]
    fn contrast_color_picks_the_ink_that_reads() {
        assert_eq!(
            parse("contrast-color(#000)", true),
            Some(Rgba::rgb(255, 255, 255))
        );
        assert_eq!(
            parse("contrast-color(#fff)", true),
            Some(Rgba::rgb(0, 0, 0))
        );
    }

    #[test]
    fn color_mix_reads_lab_and_oklab() {
        let oklab = parse("color-mix(in oklab, #000, #fff)", true).unwrap();
        assert_eq!((oklab.r, oklab.g, oklab.b), (0x63, 0x63, 0x63));
        let lab = parse("color-mix(in lab, #000, #fff)", true).unwrap();
        assert!(lab.r > oklab.r && lab.r < 140, "{lab:?}");
        let polar = parse("color-mix(in oklch, #ff0000, #0000ff)", true).unwrap();
        assert!(polar.r > 80 && polar.b > 80, "{polar:?}");
        let hsl = parse("color-mix(in hsl, #ff0000, #00ff00)", true).unwrap();
        assert!(hsl.r > 100 && hsl.g > 100, "{hsl:?}");
    }
}
