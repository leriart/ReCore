//! Remove optional container metadata without decoding the image or changing
//! the original file. Keep the color profile and the orientation needed to
//! display the image correctly.

use std::ops::Range;

use anyhow::{Context, Result};

const PNG: &[u8] = b"\x89PNG\r\n\x1a\n";

#[derive(Clone, Copy, Default)]
pub struct AttachmentPrivacy {
    pub strip_image_metadata: bool,
    pub anonymize_names: bool,
}

impl AttachmentPrivacy {
    pub(crate) fn enabled(self) -> bool {
        self.strip_image_metadata || self.anonymize_names
    }

    pub(crate) fn prepare(self, name: String, mut data: Vec<u8>) -> Result<(String, Vec<u8>)> {
        if self.strip_image_metadata {
            let length = if data.starts_with(b"\xff\xd8") {
                jpeg(&mut data)
            } else if data.starts_with(PNG) {
                png(&mut data)
            } else if data.get(..4) == Some(b"RIFF") && data.get(8..12) == Some(b"WEBP") {
                webp(&mut data)
            } else {
                Some(data.len())
            };
            let length = length.context(
                "ReCore could not remove this image's metadata. Export a new copy and try again.",
            )?;
            data.truncate(length);
        }
        let name = if self.anonymize_names {
            anonymous_name(&name, rand::random())
        } else {
            name
        };
        Ok((name, data))
    }
}

fn anonymous_name(name: &str, id: u64) -> String {
    let spoiler = if name.starts_with("SPOILER_") {
        "SPOILER_"
    } else {
        ""
    };
    let original = name.strip_prefix("SPOILER_").unwrap_or(name);
    let mut name = format!("{spoiler}{id:016x}");
    if let Some((stem, extension)) = original.rsplit_once('.')
        && !stem.is_empty()
        && !extension.is_empty()
        && extension.len() <= 16
        && extension.bytes().all(|byte| byte.is_ascii_alphanumeric())
    {
        name.push('.');
        name.push_str(extension);
    }
    name
}

fn keep(data: &mut [u8], write: &mut usize, range: Range<usize>) {
    let length = range.len();
    data.copy_within(range, *write);
    *write += length;
}

fn append(data: &mut [u8], write: &mut usize, bytes: &[u8]) {
    data[*write..*write + bytes.len()].copy_from_slice(bytes);
    *write += bytes.len();
}

fn orientation(data: &[u8]) -> Option<u8> {
    let tiff = data.strip_prefix(b"Exif\0\0").unwrap_or(data);
    let little = match tiff.get(..4)? {
        b"II\x2a\0" => true,
        b"MM\0\x2a" => false,
        _ => return None,
    };
    let short = |bytes: &[u8]| {
        let bytes = [bytes[0], bytes[1]];
        if little {
            u16::from_le_bytes(bytes)
        } else {
            u16::from_be_bytes(bytes)
        }
    };
    let long = |bytes: &[u8]| {
        let bytes = [bytes[0], bytes[1], bytes[2], bytes[3]];
        if little {
            u32::from_le_bytes(bytes)
        } else {
            u32::from_be_bytes(bytes)
        }
    };
    let offset = usize::try_from(long(tiff.get(4..8)?)).ok()?;
    let table = tiff.get(offset..)?;
    let count = usize::from(short(table.get(..2)?));
    table[2..]
        .as_chunks::<12>()
        .0
        .iter()
        .take(count)
        .find_map(|entry| {
            if short(entry) == 0x0112 && short(&entry[2..]) == 3 && long(&entry[4..]) == 1 {
                let value = short(&entry[8..]);
                (2..=8).contains(&value).then_some(value as u8)
            } else {
                None
            }
        })
}

fn orientation_tiff(value: u8) -> [u8; 26] {
    [
        b'I', b'I', 42, 0, 8, 0, 0, 0, 1, 0, 0x12, 0x01, 3, 0, 1, 0, 0, 0, value, 0, 0, 0, 0, 0, 0,
        0,
    ]
}

/// Inside a scan, FF00 escapes a byte and FFD0–FFD7 restart the same scan.
/// A real marker can follow several FF padding bytes.
fn jpeg_marker(data: &[u8], mut at: usize, scan: bool) -> Option<(usize, usize, u8)> {
    loop {
        if scan {
            at += data.get(at..)?.iter().position(|byte| *byte == 0xff)?;
        }
        let start = at;
        if *data.get(at)? != 0xff {
            return None;
        }
        while data.get(at) == Some(&0xff) {
            at += 1;
        }
        let tag = *data.get(at)?;
        at += 1;
        if !scan || !matches!(tag, 0 | 0xd0..=0xd7) {
            return Some((start, at, tag));
        }
    }
}

fn jpeg(data: &mut [u8]) -> Option<usize> {
    let mut read = 2;
    let mut write = 2;
    let mut scan = false;
    let mut has_scan = false;
    let mut keeps_orientation = false;
    loop {
        let (start, size_at, tag) = jpeg_marker(data, read, scan)?;
        if start > read {
            keep(data, &mut write, read..start);
        }
        match tag {
            0xd9 if has_scan => {
                append(data, &mut write, b"\xff\xd9");
                return Some(write);
            }
            0x01 => {
                keep(data, &mut write, start..size_at);
                read = size_at;
                continue;
            }
            0x00 | 0xd0..=0xd9 => return None,
            _ => {}
        }
        let length = usize::from(u16::from_be_bytes(
            data.get(size_at..size_at + 2)?.try_into().ok()?,
        ));
        if length < 2 {
            return None;
        }
        let end = size_at + length;
        let payload = data.get(size_at + 2..end)?;
        match tag {
            0xe0 if payload.len() >= 14 && payload.starts_with(b"JFIF\0") => {
                let mut jfif = [0; 18];
                jfif[..4].copy_from_slice(b"\xff\xe0\x00\x10");
                jfif[4..16].copy_from_slice(&payload[..12]);
                append(data, &mut write, &jfif);
            }
            0xe1 if !keeps_orientation
                && payload.len() >= 32
                && payload.starts_with(b"Exif\0\0") =>
            {
                if let Some(value) = orientation(payload) {
                    append(data, &mut write, b"\xff\xe1\x00\x22Exif\0\0");
                    append(data, &mut write, &orientation_tiff(value));
                    keeps_orientation = true;
                }
            }
            0xe2 if payload.starts_with(b"ICC_PROFILE\0") => keep(data, &mut write, start..end),
            0xee if payload.len() == 12 && payload.starts_with(b"Adobe") => {
                keep(data, &mut write, start..end);
            }
            0xe0..=0xef | 0xfe => {}
            _ => keep(data, &mut write, start..end),
        }
        scan = tag == 0xda || (scan && tag == 0xdc);
        has_scan |= scan;
        read = end;
    }
}

fn png(data: &mut [u8]) -> Option<usize> {
    let mut read = PNG.len();
    let mut write = read;
    let mut keeps_orientation = false;
    let mut has_image = false;
    loop {
        let header = data.get(read..read.checked_add(8)?)?;
        let length = u32::from_be_bytes(header[..4].try_into().ok()?) as usize;
        let kind: [u8; 4] = header[4..8].try_into().ok()?;
        let end = read.checked_add(12)?.checked_add(length)?;
        let payload = data.get(read + 8..end.checked_sub(4)?)?;
        data.get(end - 4..end)?;
        if read == PNG.len() && (kind != *b"IHDR" || length != 13) {
            return None;
        }
        match &kind {
            b"eXIf" if !keeps_orientation => {
                if let Some(value) = orientation(payload)
                    && length >= 26
                {
                    let mut chunk = [0; 38];
                    chunk[..4].copy_from_slice(&26u32.to_be_bytes());
                    chunk[4..8].copy_from_slice(b"eXIf");
                    chunk[8..34].copy_from_slice(&orientation_tiff(value));
                    let crc = crc32fast::hash(&chunk[4..34]);
                    chunk[34..].copy_from_slice(&crc.to_be_bytes());
                    append(data, &mut write, &chunk);
                    keeps_orientation = true;
                }
            }
            b"IEND" => {
                if length != 0 || !has_image {
                    return None;
                }
                keep(data, &mut write, read..end);
                return Some(write);
            }
            b"IDAT" => {
                has_image = true;
                keep(data, &mut write, read..end);
            }
            b"IHDR" | b"PLTE" | b"bKGD" | b"cHRM" | b"cICP" | b"gAMA" | b"iCCP" | b"pHYs"
            | b"sBIT" | b"sRGB" | b"tRNS" | b"acTL" | b"fcTL" | b"fdAT" => {
                keep(data, &mut write, read..end);
            }
            _ if kind[0].is_ascii_uppercase() => return None,
            _ => {}
        }
        read = end;
    }
}

fn webp(data: &mut [u8]) -> Option<usize> {
    let end = 8usize.checked_add(u32::from_le_bytes(data.get(4..8)?.try_into().ok()?) as usize)?;
    if end > data.len() || end < 12 {
        return None;
    }
    let mut read = 12;
    let mut write = 12;
    let mut flags = None;
    let mut keeps_orientation = false;
    let mut has_image = false;
    while read < end {
        let (kind, payload_end, next) = riff_chunk(data, read, end)?;
        let length = payload_end - read - 8;
        match &kind {
            b"VP8X" if length == 10 && flags.is_none() && read == 12 => {
                flags = Some(write + 8);
                keep(data, &mut write, read..next);
            }
            b"VP8X" => return None,
            b"EXIF" if !keeps_orientation && flags.is_some() => {
                if let Some(value) = orientation(&data[read + 8..payload_end])
                    && length >= 26
                {
                    append(data, &mut write, b"EXIF\x1a\0\0\0");
                    append(data, &mut write, &orientation_tiff(value));
                    keeps_orientation = true;
                }
            }
            b"VP8 " | b"VP8L" => {
                has_image = true;
                keep(data, &mut write, read..next);
            }
            b"ANMF" => {
                webp_frame(data, read, payload_end, &mut write)?;
                has_image = true;
            }
            b"ANIM" | b"ALPH" | b"ICCP" => keep(data, &mut write, read..next),
            _ => {}
        }
        read = next;
    }
    if !has_image {
        return None;
    }
    if let Some(flags) = flags {
        // VP8X advertises which optional chunks remain.
        data[flags] &= !0x0c;
        data[flags] |= if keeps_orientation { 0x08 } else { 0 };
    }
    data[4..8].copy_from_slice(&((write - 8) as u32).to_le_bytes());
    Some(write)
}

fn riff_chunk(data: &[u8], start: usize, limit: usize) -> Option<([u8; 4], usize, usize)> {
    let body = start.checked_add(8)?;
    let header = data.get(start..body)?;
    let kind = header[..4].try_into().ok()?;
    let length = u32::from_le_bytes(header[4..].try_into().ok()?) as usize;
    let end = body.checked_add(length)?;
    let next = end.checked_add(length % 2)?;
    (next <= limit).then_some((kind, end, next))
}

fn webp_frame(data: &mut [u8], start: usize, end: usize, write: &mut usize) -> Option<()> {
    // ANMF has 16 bytes of frame geometry and timing before its own RIFF chunks.
    let mut read = start.checked_add(24)?;
    if read > end {
        return None;
    }
    let output = *write;
    keep(data, write, start..read);
    let mut has_image = false;
    while read < end {
        let (kind, _, next) = riff_chunk(data, read, end)?;
        match &kind {
            b"VP8 " | b"VP8L" => {
                has_image = true;
                keep(data, write, read..next);
            }
            b"ALPH" => keep(data, write, read..next),
            _ => {}
        }
        read = next;
    }
    if !has_image {
        return None;
    }
    let length = u32::try_from(*write - output - 8).ok()?;
    data[output + 4..output + 8].copy_from_slice(&length.to_le_bytes());
    Some(())
}

#[cfg(test)]
mod tests {
    use super::*;

    const PRIVATE: &[u8] = b"author, location and serial number";
    const STRIP: AttachmentPrivacy = AttachmentPrivacy {
        strip_image_metadata: true,
        anonymize_names: false,
    };

    fn keep_image_bytes(data: Vec<u8>, start: &[u8], end: &[u8]) -> Vec<u8> {
        let (_, clean) = STRIP.prepare("test".into(), data).unwrap();
        assert!(clean.starts_with(start));
        assert!(clean.ends_with(end));
        assert!(!clean.windows(PRIVATE.len()).any(|bytes| bytes == PRIVATE));
        assert_eq!(
            STRIP.prepare("test".into(), clean.clone()).unwrap().1,
            clean
        );
        clean
    }

    fn jpeg_segment(tag: u8, contents: &[u8], data: &mut Vec<u8>) {
        data.extend([0xff, tag]);
        data.extend(u16::try_from(contents.len() + 2).unwrap().to_be_bytes());
        data.extend(contents);
    }

    fn png_chunk(kind: &[u8; 4], contents: &[u8], data: &mut Vec<u8>) {
        data.extend(u32::try_from(contents.len()).unwrap().to_be_bytes());
        let crc_start = data.len();
        data.extend(kind);
        data.extend(contents);
        data.extend(crc32fast::hash(&data[crc_start..]).to_be_bytes());
    }

    fn webp_chunk(kind: &[u8; 4], contents: &[u8], data: &mut Vec<u8>) {
        data.extend(kind);
        data.extend(u32::try_from(contents.len()).unwrap().to_le_bytes());
        data.extend(contents);
        if !contents.len().is_multiple_of(2) {
            data.push(0);
        }
    }

    #[test]
    fn strips_jpeg_metadata_before_and_after_the_scan() {
        let mut image = b"\xff\xd8".to_vec();
        let mut exif = b"Exif\0\0".to_vec();
        exif.extend(orientation_tiff(6));
        exif.extend(PRIVATE);
        jpeg_segment(0xe1, &exif, &mut image);
        jpeg_segment(0xe1, PRIVATE, &mut image);
        jpeg_segment(0xed, PRIVATE, &mut image);
        jpeg_segment(0xe2, b"ICC_PROFILE\0\x01\x01test", &mut image);
        jpeg_segment(0xda, b"", &mut image);
        image.extend_from_slice(b"\x11\xff\x00\x22\xff\xd1");
        jpeg_segment(0xfe, PRIVATE, &mut image);
        jpeg_segment(0xda, b"", &mut image);
        image.extend_from_slice(b"\x33\xff\xd9");
        image.extend(PRIVATE);

        let clean = keep_image_bytes(image, b"\xff\xd8", b"\x33\xff\xd9");
        assert_eq!(orientation(&clean[6..38]), Some(6));
        assert!(clean.windows(12).any(|bytes| bytes == b"ICC_PROFILE\0"));
        assert!(
            clean
                .windows(6)
                .any(|bytes| bytes == b"\x11\xff\x00\x22\xff\xd1")
        );
    }

    #[test]
    fn strips_png_text_and_keeps_image_data_and_orientation() {
        let mut image = PNG.to_vec();
        png_chunk(b"IHDR", &[0; 13], &mut image);
        png_chunk(b"iCCP", b"profile", &mut image);
        let mut exif = orientation_tiff(8).to_vec();
        exif.extend(PRIVATE);
        png_chunk(b"eXIf", &exif, &mut image);
        png_chunk(b"IDAT", b"image samples", &mut image);
        for kind in [b"tEXt", b"zTXt", b"iTXt", b"tIME"] {
            png_chunk(kind, PRIVATE, &mut image);
        }
        png_chunk(b"IEND", b"", &mut image);
        image.extend(PRIVATE);

        let clean = keep_image_bytes(image, PNG, b"IEND\xae\x42\x60\x82");
        let exif = clean.windows(4).position(|bytes| bytes == b"eXIf").unwrap();
        assert_eq!(orientation(&clean[exif + 4..exif + 30]), Some(8));
        assert_eq!(
            &clean[exif + 30..exif + 34],
            &crc32fast::hash(&clean[exif..exif + 30]).to_be_bytes()
        );
        assert!(clean.windows(17).any(|bytes| bytes == b"IDATimage samples"));
    }

    #[test]
    fn strips_webp_xmp_and_updates_the_riff_length_and_flags() {
        let mut image = b"RIFF\0\0\0\0WEBP".to_vec();
        webp_chunk(b"VP8X", &[0x2c, 0, 0, 0, 1, 0, 0, 1, 0, 0], &mut image);
        webp_chunk(b"ICCP", b"profile", &mut image);
        webp_chunk(b"VP8L", b"image!", &mut image);
        let mut exif = orientation_tiff(3).to_vec();
        exif.extend(PRIVATE);
        webp_chunk(b"EXIF", &exif, &mut image);
        webp_chunk(b"XMP ", PRIVATE, &mut image);
        let size = u32::try_from(image.len() - 8).unwrap();
        image[4..8].copy_from_slice(&size.to_le_bytes());
        image.extend(PRIVATE);

        let clean = keep_image_bytes(image, b"RIFF", &[0, 0, 0, 0]);
        assert_eq!(clean[20], 0x28);
        assert_eq!(
            clean.len() - 8,
            u32::from_le_bytes(clean[4..8].try_into().unwrap()) as usize
        );
        assert_eq!(orientation(&clean[clean.len() - 26..]), Some(3));
        assert!(clean.windows(6).any(|bytes| bytes == b"image!"));
    }

    #[test]
    fn refuses_truncated_recognized_images_and_passes_other_files() {
        for data in [b"\xff\xd8\xff".as_slice(), PNG, b"RIFF\xff\xff\xff\xffWEBP"] {
            assert!(STRIP.prepare("test".into(), data.to_vec()).is_err());
        }
        let (_, untouched) = STRIP
            .prepare("report.pdf".into(), PRIVATE.to_vec())
            .unwrap();
        assert_eq!(untouched, PRIVATE);
    }

    #[test]
    fn keeps_spoilers_and_extensions_without_leaking_old_names() {
        assert_eq!(
            anonymous_name("SPOILER_family.JPG", 1),
            "SPOILER_0000000000000001.JPG"
        );
        assert_eq!(anonymous_name(".profile", 2), "0000000000000002");
        assert_eq!(
            anonymous_name("report.private location", 3),
            "0000000000000003"
        );
    }
}
