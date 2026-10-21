//! GIMP palette files (`.gpl`): the plain-text swatch format GIMP, Krita, Inkscape, Aseprite and
//! many pixel-art tools share.
//!
//! ```text
//! GIMP Palette
//! Name: Sunset
//! Columns: 4
//! #
//! 255 128   0  Orange
//! ```
//!
//! After the header, `Key: value` lines (`Name`, `Columns`, GIMP 3's `Channels`) and `#` comments,
//! each colour is three 0..255 numbers and an optional name, which may hold spaces. There are no
//! groups and no colour model other than RGB.

use crate::error::{PsdError, Result};

/// The first bytes of every `.gpl` file.
pub const SIGNATURE: &[u8] = b"GIMP Palette";
/// Most colours read from one file.
pub const MAX_COLORS: usize = 100_000;

/// One palette entry.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GplColor {
    /// 0..255 red, green, blue.
    pub rgb: [u8; 3],
    /// The name after the numbers; empty when the entry has none.
    pub name: String,
}

/// A parsed palette.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct GplPalette {
    /// The `Name:` line; empty when absent.
    pub name: String,
    /// The colours, in file order.
    pub colors: Vec<GplColor>,
}

fn channel(token: &str) -> Option<u8> {
    let v: f32 = token.parse().ok()?;
    v.is_finite().then(|| v.round().clamp(0.0, 255.0) as u8)
}

/// Splits a colour line into its three channels and the rest of the line (the name).
fn colour_line(line: &str) -> Option<([u8; 3], &str)> {
    let mut rest = line;
    let mut rgb = [0u8; 3];
    for c in &mut rgb {
        rest = rest.trim_start();
        let end = rest.find(char::is_whitespace).unwrap_or(rest.len());
        *c = channel(rest.get(..end)?)?;
        rest = rest.get(end..)?;
    }
    Some((rgb, rest.trim()))
}

/// Parses a `.gpl` file.
pub fn parse(data: &[u8]) -> Result<GplPalette> {
    let text = String::from_utf8_lossy(data);
    let text = text.strip_prefix('\u{feff}').unwrap_or(&text);
    let mut lines = text.lines();
    if !lines.next().is_some_and(|l| l.as_bytes().starts_with(SIGNATURE)) {
        return Err(PsdError::invalid("not a GIMP palette file (no `GIMP Palette` header)"));
    }
    let mut out = GplPalette::default();
    for line in lines {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        if let Some(name) = line.strip_prefix("Name:") {
            out.name = name.trim().to_string();
        } else if let Some((rgb, name)) = colour_line(line) {
            if out.colors.len() >= MAX_COLORS {
                return Err(PsdError::LimitExceeded("too many palette colours"));
            }
            out.colors.push(GplColor { rgb, name: name.to_string() });
        } else if !line.split_whitespace().next().is_some_and(|k| k.ends_with(':')) {
            return Err(PsdError::invalid(format!("unreadable palette line `{}`", line.chars().take(40).collect::<String>())));
        }
    }
    if out.colors.is_empty() {
        return Err(PsdError::invalid("the palette holds no colours"));
    }
    Ok(out)
}

/// Writes a palette (at most [`MAX_COLORS`] entries are written).
pub fn write(palette: &GplPalette) -> Vec<u8> {
    let one_line = |s: &str| s.replace(['\r', '\n'], " ");
    let mut out = format!("GIMP Palette\nName: {}\nColumns: 0\n#\n", one_line(&palette.name));
    for c in palette.colors.iter().take(MAX_COLORS) {
        let name = one_line(&c.name);
        let name = name.trim();
        if name.is_empty() {
            out.push_str(&format!("{:3} {:3} {:3}\n", c.rgb[0], c.rgb[1], c.rgb[2]));
        } else {
            out.push_str(&format!("{:3} {:3} {:3}\t{name}\n", c.rgb[0], c.rgb[1], c.rgb[2]));
        }
    }
    out.into_bytes()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_header_comments_and_names() {
        let text = "GIMP Palette\nName: Sunset\nColumns: 4\n#\n# a comment\n255 128   0\tOrange\n  0   0   0\n10 20 30 Dark teal blue\n";
        let p = parse(text.as_bytes()).unwrap();
        assert_eq!(p.name, "Sunset");
        assert_eq!(
            p.colors,
            vec![
                GplColor { rgb: [255, 128, 0], name: "Orange".into() },
                GplColor { rgb: [0, 0, 0], name: String::new() },
                GplColor { rgb: [10, 20, 30], name: "Dark teal blue".into() },
            ]
        );
    }

    #[test]
    fn tolerates_bom_crlf_gimp3_channels_and_float_values() {
        let text = "\u{feff}GIMP Palette\r\nName: X\r\nChannels: RGB\r\n#\r\n300 -4 127.6 Odd\r\n";
        let p = parse(text.as_bytes()).unwrap();
        assert_eq!(p.colors, vec![GplColor { rgb: [255, 0, 128], name: "Odd".into() }]);
    }

    #[test]
    fn write_then_parse_round_trips() {
        let p = GplPalette {
            name: "Mine".into(),
            colors: vec![GplColor { rgb: [1, 2, 3], name: "A\nB".into() }, GplColor { rgb: [255, 255, 255], name: String::new() }],
        };
        let back = parse(&write(&p)).unwrap();
        assert_eq!(back.name, "Mine");
        assert_eq!(back.colors, vec![GplColor { rgb: [1, 2, 3], name: "A B".into() }, GplColor { rgb: [255, 255, 255], name: String::new() }]);
    }

    #[test]
    fn malformed_files_are_errors_not_panics() {
        for bad in
            ["", "GIMP Palette\n", "GIMP Palette\nName: only\n#\n", "GIMP Palette\n1 2\n", "GIMP Palette\n1 2 x\n", "GIMP Palette\nNaN 1 2\n", "nope\n1 2 3\n"]
        {
            assert!(parse(bad.as_bytes()).is_err(), "{bad:?}");
        }
        assert!(parse(&[0xff, 0xfe, 0xfd]).is_err());
        assert!(parse("GIMP Palette\n9é 1 2\n".as_bytes()).is_err());
    }

    #[test]
    fn too_many_colours_are_refused() {
        let mut text = String::from("GIMP Palette\n");
        for _ in 0..=MAX_COLORS {
            text.push_str("1 2 3\n");
        }
        assert!(matches!(parse(text.as_bytes()), Err(PsdError::LimitExceeded(_))));
    }
}
