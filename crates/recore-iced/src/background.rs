//! The grade ReCore puts over a background picture, GIF or video: the
//! filter presets from Appearance, the brightness slider and the theme-aware
//! dim. The grade is applied to the first frame too, so a still background
//! and the first frame of an animation look alike.
//!
//! The dim follows Ambxst's `palette.frag`: instead of laying a flat black
//! sheet over the picture, it weights the theme's own colours by their
//! distance to each pixel and mixes that palette colour in. The picture keeps
//! its shapes while settling into the palette around it. A 32×32×32 lookup
//! table is built when the theme or dim changes and reused for every frame,
//! so the per-pixel work is one small lookup rather than a loop over the
//! palette.

/// A background filter, as Appearance offers it.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Filter {
    #[default]
    None,
    Warm,
    Cool,
    Mono,
    Sepia,
    Scanlines,
}

impl Filter {
    pub const ALL: [Self; 6] = [
        Self::None,
        Self::Warm,
        Self::Cool,
        Self::Mono,
        Self::Sepia,
        Self::Scanlines,
    ];

    pub fn from_name(name: &str) -> Self {
        match name {
            "warm" => Self::Warm,
            "cool" => Self::Cool,
            "mono" => Self::Mono,
            "sepia" => Self::Sepia,
            "scanlines" => Self::Scanlines,
            _ => Self::None,
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Self::None => "none",
            Self::Warm => "warm",
            Self::Cool => "cool",
            Self::Mono => "mono",
            Self::Sepia => "sepia",
            Self::Scanlines => "scanlines",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::None => "None",
            Self::Warm => "Warm",
            Self::Cool => "Cool",
            Self::Mono => "Mono",
            Self::Sepia => "Sepia",
            Self::Scanlines => "Scanlines",
        }
    }
}

/// The most theme colours the palette dim weights at once.
const PALETTE: usize = 12;
/// Each channel of the palette lookup table is expected at 5 bits.
const LUT_SIDE: usize = 32;
const LUT_CELLS: usize = LUT_SIDE * LUT_SIDE * LUT_SIDE;

/// How a picture's channels are changed before it is drawn. The default is
/// the identity, so animated pictures elsewhere in the window are untouched.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Grade {
    brightness: f32,
    contrast: f32,
    saturation: f32,
    sepia: f32,
    red: f32,
    green: f32,
    blue: f32,
    scanline: f32,
    palette: [[u8; 3]; PALETTE],
    palette_len: u8,
    dim: f32,
}

impl Default for Grade {
    fn default() -> Self {
        Self {
            brightness: 1.0,
            contrast: 1.0,
            saturation: 1.0,
            sepia: 0.0,
            red: 1.0,
            green: 1.0,
            blue: 1.0,
            scanline: 0.0,
            palette: [[0; 3]; PALETTE],
            palette_len: 0,
            dim: 0.0,
        }
    }
}

impl Grade {
    pub fn new(filter: Filter, brightness: f32) -> Self {
        let mut grade = Self {
            brightness: brightness.clamp(0.25, 2.0),
            ..Self::default()
        };
        match filter {
            Filter::None => {}
            Filter::Warm => {
                grade.saturation = 0.9;
                grade.sepia = 0.18;
                grade.red = 1.10;
                grade.blue = 0.90;
            }
            Filter::Cool => {
                grade.saturation = 0.9;
                grade.red = 0.88;
                grade.blue = 1.12;
            }
            Filter::Mono => {
                grade.saturation = 0.0;
                grade.contrast = 1.08;
            }
            Filter::Sepia => {
                grade.saturation = 0.55;
                grade.sepia = 0.82;
                grade.contrast = 1.04;
            }
            Filter::Scanlines => {
                grade.scanline = 0.18;
                grade.contrast = 1.03;
            }
        }
        grade
    }

    /// Mixes each pixel towards the nearest colours of `palette`, as
    /// Ambxst's wallpaper shader does. `dim` is how much of the mapped
    /// colour wins over the picture, 0..1; zero leaves the picture alone.
    pub fn with_palette<I>(mut self, palette: I, dim: f32) -> Self
    where
        I: IntoIterator<Item = [u8; 3]>,
    {
        let mut len = 0;
        for (at, colour) in palette.into_iter().take(PALETTE).enumerate() {
            self.palette[at] = colour;
            len = at + 1;
        }
        self.palette_len = len as u8;
        self.dim = dim.clamp(0.0, 1.0);
        self
    }

    pub fn is_identity(self) -> bool {
        if self.palette_len > 0 && self.dim > 0.0 {
            return false;
        }
        Self {
            palette: [[0; 3]; PALETTE],
            palette_len: 0,
            dim: 0.0,
            ..self
        } == Self::default()
    }
}

/// The palette lookup table for the grade showing at the moment. A still
/// picture builds it once per theme change; an animated background reuses it
/// for every frame, which keeps the dim from turning into a per-frame
/// palette search.
fn palette_lut(grade: &Grade) -> std::sync::Arc<Vec<u8>> {
    type Key = ([[u8; 3]; PALETTE], u8, u32);
    type Cache = std::sync::Mutex<Option<(Key, std::sync::Arc<Vec<u8>>)>>;
    static CACHE: std::sync::OnceLock<Cache> = std::sync::OnceLock::new();
    let key = (grade.palette, grade.palette_len, grade.dim.to_bits());
    let cache = CACHE.get_or_init(|| std::sync::Mutex::new(None));
    let mut cache = cache
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    if let Some((cached, table)) = cache.as_ref()
        && cached == &key
    {
        return table.clone();
    }
    let table = std::sync::Arc::new(palette_table(grade));
    *cache = Some((key, table.clone()));
    table
}

/// Builds the quantised table. Kept separate so the cache above owns the
/// result for the frames that follow.
fn palette_table(grade: &Grade) -> Vec<u8> {
    let colours: Vec<[f32; 3]> = grade.palette[..usize::from(grade.palette_len)]
        .iter()
        .map(|colour| {
            [
                f32::from(colour[0]) / 255.0,
                f32::from(colour[1]) / 255.0,
                f32::from(colour[2]) / 255.0,
            ]
        })
        .collect();
    let mut lut = vec![0u8; LUT_CELLS * 3];
    if colours.is_empty() {
        return lut;
    }
    // Ambxst keeps colours close to the palette while allowing gradients with
    // a sharpness of 20; the same value keeps a photograph's identity.
    const SHARPNESS: f32 = 20.0;
    for red in 0..LUT_SIDE {
        for green in 0..LUT_SIDE {
            for blue in 0..LUT_SIDE {
                let sample = [
                    (red as f32 + 0.5) / LUT_SIDE as f32,
                    (green as f32 + 0.5) / LUT_SIDE as f32,
                    (blue as f32 + 0.5) / LUT_SIDE as f32,
                ];
                let mut sum = [0.0f32; 3];
                let mut total = 0.0f32;
                let mut nearest = colours[0];
                let mut nearest_distance = f32::MAX;
                for colour in &colours {
                    let distance = (sample[0] - colour[0]).powi(2)
                        + (sample[1] - colour[1]).powi(2)
                        + (sample[2] - colour[2]).powi(2);
                    if distance < nearest_distance {
                        nearest = *colour;
                        nearest_distance = distance;
                    }
                    let weight = (-SHARPNESS * distance).exp();
                    for channel in 0..3 {
                        sum[channel] += colour[channel] * weight;
                    }
                    total += weight;
                }
                if total <= f32::EPSILON {
                    sum = nearest;
                    total = 1.0;
                }
                let at = ((red * LUT_SIDE + green) * LUT_SIDE + blue) * 3;
                for channel in 0..3 {
                    lut[at + channel] =
                        ((sum[channel] / total).clamp(0.0, 1.0) * 255.0).round() as u8;
                }
            }
        }
    }
    lut
}

/// Applies `grade` in place to straight RGBA pixels. `width` is the
/// picture's width, so the scanline filter can draw one line every third
/// row rather than every third pixel.
///
/// The work is fixed point: 256-entry tone tables and a 3x3 matrix, all
/// scaled by 4096, instead of a float chain for every channel of every
/// pixel. The filter, contrast, saturation, sepia and channel gains stay
/// as they were; only the last bit of rounding can differ.
pub fn apply(rgba: &mut [u8], width: u32, grade: Grade) {
    if grade.is_identity() {
        return;
    }
    const SHIFT: u32 = 12;
    const ONE: i32 = 1 << SHIFT;
    const HALF: i32 = ONE / 2;
    let tone = std::array::from_fn::<i32, 256, _>(|value| {
        let channel = value as f32 / 255.0;
        (((channel - 0.5) * grade.contrast + 0.5) * grade.brightness * ONE as f32).round() as i32
    });
    let matrix = |scan: f32| {
        let (s, p) = (grade.saturation, grade.sepia);
        let luma = [0.299, 0.587, 0.114];
        // Saturation first, then sepia, as one linear map.
        let sat = |row: usize, column: usize| {
            (if row == column { s } else { 0.0 }) + (1.0 - s) * luma[column]
        };
        let sepia = |row: usize, column: usize| {
            let paper = [
                [0.393, 0.769, 0.189],
                [0.349, 0.686, 0.168],
                [0.272, 0.534, 0.131],
            ];
            (if row == column { 1.0 - p } else { 0.0 }) + p * paper[row][column]
        };
        let gain = [grade.red * scan, grade.green * scan, grade.blue * scan];
        std::array::from_fn::<[i32; 3], 3, _>(|row| {
            std::array::from_fn(|column| {
                let mut sum = 0.0;
                for inner in 0..3 {
                    sum += sepia(row, inner) * sat(inner, column);
                }
                (sum * gain[row] * ONE as f32).round() as i32
            })
        })
    };
    let plain = matrix(1.0);
    let scanned = (grade.scanline > 0.0).then(|| matrix(1.0 - grade.scanline));
    let diagonal = grade.saturation == 1.0 && grade.sepia == 0.0;
    let palette = (grade.palette_len > 0 && grade.dim > 0.0).then(|| palette_lut(&grade));
    let dim = grade.dim.clamp(0.0, 1.0);
    let width = width.max(1) as usize;
    let row_width = (width * 4).max(4);
    for (row, line) in rgba.chunks_mut(row_width).enumerate() {
        let m = match &scanned {
            Some(scanned) if row % 3 == 0 => scanned,
            _ => &plain,
        };
        for pixel in line.chunks_exact_mut(4) {
            let (red, green, blue) = (
                tone[pixel[0] as usize],
                tone[pixel[1] as usize],
                tone[pixel[2] as usize],
            );
            let (mut out_red, mut out_green, mut out_blue) = (0, 0, 0);
            if diagonal {
                out_red = (m[0][0] * red + HALF) >> SHIFT;
                out_green = (m[1][1] * green + HALF) >> SHIFT;
                out_blue = (m[2][2] * blue + HALF) >> SHIFT;
            } else {
                for (at, out) in [&mut out_red, &mut out_green, &mut out_blue]
                    .into_iter()
                    .enumerate()
                {
                    *out = (m[at][0] * red + m[at][1] * green + m[at][2] * blue + HALF) >> SHIFT;
                }
            }
            let ink = |value: i32| {
                let value = value.clamp(0, ONE);
                ((value * 255 + HALF) >> SHIFT) as u8
            };
            let (mut red, mut green, mut blue) = (ink(out_red), ink(out_green), ink(out_blue));
            if let Some(lut) = &palette {
                let at = ((usize::from(red >> 3) * LUT_SIDE + usize::from(green >> 3)) * LUT_SIDE
                    + usize::from(blue >> 3))
                    * 3;
                let mapped = [lut[at], lut[at + 1], lut[at + 2]];
                let mix = |original: u8, mapped: u8| {
                    (f32::from(original) * (1.0 - dim) + f32::from(mapped) * dim)
                        .round()
                        .clamp(0.0, 255.0) as u8
                };
                red = mix(red, mapped[0]);
                green = mix(green, mapped[1]);
                blue = mix(blue, mapped[2]);
            }
            pixel[0] = red;
            pixel[1] = green;
            pixel[2] = blue;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The float pipeline the fixed point one replaced.
    fn reference(rgba: &mut [u8], width: u32, grade: Grade) {
        let width = width.max(1) as usize;
        for (index, pixel) in rgba.chunks_exact_mut(4).enumerate() {
            let mut red = f32::from(pixel[0]) / 255.0;
            let mut green = f32::from(pixel[1]) / 255.0;
            let mut blue = f32::from(pixel[2]) / 255.0;
            red = ((red - 0.5) * grade.contrast + 0.5) * grade.brightness;
            green = ((green - 0.5) * grade.contrast + 0.5) * grade.brightness;
            blue = ((blue - 0.5) * grade.contrast + 0.5) * grade.brightness;
            if grade.saturation != 1.0 {
                let luma = red * 0.299 + green * 0.587 + blue * 0.114;
                red = luma + (red - luma) * grade.saturation;
                green = luma + (green - luma) * grade.saturation;
                blue = luma + (blue - luma) * grade.saturation;
            }
            if grade.sepia > 0.0 {
                let (r, g, b) = (red, green, blue);
                red = r * (1.0 - grade.sepia) + (r * 0.393 + g * 0.769 + b * 0.189) * grade.sepia;
                green = g * (1.0 - grade.sepia) + (r * 0.349 + g * 0.686 + b * 0.168) * grade.sepia;
                blue = b * (1.0 - grade.sepia) + (r * 0.272 + g * 0.534 + b * 0.131) * grade.sepia;
            }
            let scan = if grade.scanline > 0.0 && (index / width).is_multiple_of(3) {
                1.0 - grade.scanline
            } else {
                1.0
            };
            red *= grade.red * scan;
            green *= grade.green * scan;
            blue *= grade.blue * scan;
            pixel[0] = (red.clamp(0.0, 1.0) * 255.0).round() as u8;
            pixel[1] = (green.clamp(0.0, 1.0) * 255.0).round() as u8;
            pixel[2] = (blue.clamp(0.0, 1.0) * 255.0).round() as u8;
        }
    }

    #[test]
    fn the_palette_dim_maps_and_blends_without_touching_the_alpha() {
        let mut pixels = vec![255, 0, 0, 255, 0, 255, 0, 255, 0, 0, 255, 255];
        let palette = [[16, 16, 20], [233, 66, 33], [0, 0, 0]];
        apply(&mut pixels, 3, Grade::default().with_palette(palette, 1.0));
        // The three source colours are close to the palette's own red and
        // dark grounds, and the alpha is unchanged.
        assert_eq!(pixels[3], 255);
        assert_eq!(pixels[7], 255);
        assert_eq!(pixels[11], 255);
        assert!(
            pixels[0] < 255 || pixels[1] > 0,
            "a red pixel may settle into the palette"
        );
        let untouched = vec![17, 34, 51, 255];
        let mut same = untouched.clone();
        apply(&mut same, 1, Grade::default().with_palette(palette, 0.0));
        assert_eq!(same, untouched);
    }

    #[test]
    fn the_fixed_point_grade_keeps_the_float_ones_look() {
        const WIDTH: u32 = 41;
        let mut pixels = Vec::new();
        for red in (0..=255).step_by(17) {
            for green in (0..=255).step_by(17) {
                for blue in (0..=255).step_by(17) {
                    pixels.extend([red as u8, green as u8, blue as u8, 255]);
                }
            }
        }
        for filter in Filter::ALL {
            for brightness in [25, 50, 100, 150, 200] {
                let grade = Grade::new(filter, f32::from(brightness as u16) / 100.0);
                let (mut fixed, mut exact) = (pixels.clone(), pixels.clone());
                apply(&mut fixed, WIDTH, grade);
                reference(&mut exact, WIDTH, grade);
                let worst = fixed
                    .iter()
                    .zip(&exact)
                    .map(|(a, b)| a.abs_diff(*b))
                    .max()
                    .unwrap_or(0);
                assert!(worst <= 1, "{filter:?} at {brightness}% differs by {worst}");
            }
        }
    }
}
