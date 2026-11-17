//! Test-only guard that reads Rust source and finds the old cream panel colours and any corner
//! radius set outside the kit, by value rather than by spelling.
//!
//! Colours: every bracketed group of three or four numbers (call arguments, arrays, struct
//! literals), every hex string and every `0xRRGGBB` literal is decoded the ways a colour can be
//! written (sRGB floats, 0-255 bytes, linear, HSL/HSV/HWB, Oklab/Oklch, Lab/Lch, XYZ), with simple
//! arithmetic and named numeric constants resolved, and compared with the old cream fills.
//!
//! Corners: `BorderRadius` and `border_radius` are bevy UI's only way to round a node; outside
//! the kit, any mention of them is a corner the kit does not own.
use bevy::color::{Color, ColorToComponents, Hsla, Hsva, Hwba, Laba, Lcha, LinearRgba, Oklaba, Oklcha, Srgba, Xyza};
use std::collections::HashMap;

/// The old cream-card fills: panel, soft panel and the two row stripes.
pub const CREAM: [(f32, f32, f32); 4] = [(0.99, 0.95, 0.87), (0.96, 0.89, 0.76), (1.0, 0.98, 0.93), (0.97, 0.91, 0.80)];
/// Per-channel tolerance: three steps of a byte.
const TOL: f32 = 3.0 / 255.0 + 1e-4;

pub fn is_cream(c: [f32; 3]) -> bool {
    CREAM.iter().any(|(r, g, b)| (c[0] - r).abs() <= TOL && (c[1] - g).abs() <= TOL && (c[2] - b).abs() <= TOL)
}

#[derive(Debug, Clone, PartialEq)]
enum Tok {
    Ident(String),
    Num(f64, bool), // value, written as an integer
    Str(String),
    Punct(char),
}

fn lex(src: &str) -> Vec<Tok> {
    let s: Vec<char> = src.chars().collect();
    let mut out = Vec::new();
    let mut i = 0;
    while i < s.len() {
        let c = s[i];
        if c.is_whitespace() {
            i += 1;
        } else if c == '/' && s.get(i + 1) == Some(&'/') {
            while i < s.len() && s[i] != '\n' {
                i += 1;
            }
        } else if c == '/' && s.get(i + 1) == Some(&'*') {
            let mut depth = 0;
            while i < s.len() {
                if s[i] == '/' && s.get(i + 1) == Some(&'*') {
                    depth += 1;
                    i += 2;
                } else if s[i] == '*' && s.get(i + 1) == Some(&'/') {
                    depth -= 1;
                    i += 2;
                    if depth == 0 {
                        break;
                    }
                } else {
                    i += 1;
                }
            }
        } else if c == 'r' && matches!(s.get(i + 1), Some('#') | Some('"')) && {
            let mut j = i + 1;
            while s.get(j) == Some(&'#') {
                j += 1;
            }
            s.get(j) == Some(&'"')
        } {
            let mut j = i + 1;
            let mut hashes = 0;
            while s[j] == '#' {
                hashes += 1;
                j += 1;
            }
            j += 1;
            let start = j;
            loop {
                if j >= s.len() {
                    break;
                }
                if s[j] == '"' && (0..hashes).all(|k| s.get(j + 1 + k) == Some(&'#')) {
                    break;
                }
                j += 1;
            }
            out.push(Tok::Str(s[start..j.min(s.len())].iter().collect()));
            i = j + 1 + hashes;
        } else if c == '"' {
            let mut j = i + 1;
            let mut v = String::new();
            while j < s.len() && s[j] != '"' {
                if s[j] == '\\' {
                    j += 1;
                }
                if j < s.len() {
                    v.push(s[j]);
                }
                j += 1;
            }
            out.push(Tok::Str(v));
            i = j + 1;
        } else if c == '\'' {
            // char literal or lifetime
            if s.get(i + 1) == Some(&'\\') {
                let mut j = i + 2;
                while j < s.len() && s[j] != '\'' {
                    j += 1;
                }
                i = j + 1;
            } else if s.get(i + 2) == Some(&'\'') {
                i += 3;
            } else {
                i += 1;
            }
        } else if c.is_ascii_digit() {
            let start = i;
            let (v, int) = if c == '0' && matches!(s.get(i + 1), Some('x') | Some('X')) {
                i += 2;
                let h0 = i;
                while i < s.len() && (s[i].is_ascii_hexdigit() || s[i] == '_') {
                    i += 1;
                }
                let h: String = s[h0..i].iter().filter(|c| **c != '_').collect();
                // suffix such as u32 / u8
                while i < s.len() && s[i].is_ascii_alphanumeric() {
                    i += 1;
                }
                (u64::from_str_radix(&h, 16).unwrap_or(0) as f64, true)
            } else {
                let mut int = true;
                while i < s.len() && (s[i].is_ascii_digit() || s[i] == '_') {
                    i += 1;
                }
                if s.get(i) == Some(&'.') && s.get(i + 1).is_none_or(|n| n.is_ascii_digit() || !(n.is_alphabetic() || *n == '_' || *n == '.')) {
                    int = false;
                    i += 1;
                    while i < s.len() && (s[i].is_ascii_digit() || s[i] == '_') {
                        i += 1;
                    }
                }
                if matches!(s.get(i), Some('e') | Some('E')) && s.get(i + 1).is_some_and(|n| n.is_ascii_digit() || *n == '-' || *n == '+') {
                    int = false;
                    i += 2;
                    while i < s.len() && s[i].is_ascii_digit() {
                        i += 1;
                    }
                }
                let text: String = s[start..i].iter().filter(|c| **c != '_').collect();
                let v = text.parse::<f64>().unwrap_or(0.0);
                // type suffix
                if i < s.len() && (s[i] == 'f' || s[i] == 'u' || s[i] == 'i') {
                    if s[i] == 'f' {
                        int = false;
                    }
                    while i < s.len() && s[i].is_ascii_alphanumeric() {
                        i += 1;
                    }
                }
                (v, int)
            };
            out.push(Tok::Num(v, int));
        } else if c.is_alphabetic() || c == '_' {
            let start = i;
            while i < s.len() && (s[i].is_alphanumeric() || s[i] == '_') {
                i += 1;
            }
            out.push(Tok::Ident(s[start..i].iter().collect()));
        } else {
            out.push(Tok::Punct(c));
            i += 1;
        }
    }
    out
}

/// `const NAME: T = <expr>;` and `let NAME = <expr>;` with a numeric value.
fn numeric_names(toks: &[Tok]) -> HashMap<String, f64> {
    let mut names = HashMap::new();
    for _ in 0..3 {
        for i in 0..toks.len() {
            let Tok::Ident(kw) = &toks[i] else { continue };
            if !matches!(kw.as_str(), "const" | "static" | "let") {
                continue;
            }
            let Some(Tok::Ident(name)) = toks.get(i + 1) else { continue };
            let Some(eq) = toks[i..].iter().position(|t| *t == Tok::Punct('=') || *t == Tok::Punct(';')).map(|p| p + i) else { continue };
            if toks[eq] != Tok::Punct('=') {
                continue;
            }
            let end = toks[eq..].iter().position(|t| *t == Tok::Punct(';')).map_or(toks.len(), |p| p + eq);
            if let Some(v) = eval(&toks[eq + 1..end], &names) {
                names.insert(name.clone(), v);
            }
        }
    }
    names
}

/// Evaluates `a + b * (c / d) as f32` over numbers and known names; `None` for anything else.
fn eval(toks: &[Tok], names: &HashMap<String, f64>) -> Option<f64> {
    // drop casts and type paths
    let mut t: Vec<&Tok> = Vec::new();
    let mut i = 0;
    while i < toks.len() {
        if toks[i] == Tok::Ident("as".into()) {
            i += 2;
            continue;
        }
        t.push(&toks[i]);
        i += 1;
    }
    let mut pos = 0;
    let v = expr(&t, &mut pos, names)?;
    (pos == t.len()).then_some(v)
}

fn expr(t: &[&Tok], pos: &mut usize, n: &HashMap<String, f64>) -> Option<f64> {
    let mut v = term(t, pos, n)?;
    while let Some(Tok::Punct(op @ ('+' | '-'))) = t.get(*pos) {
        *pos += 1;
        let r = term(t, pos, n)?;
        v = if *op == '+' { v + r } else { v - r };
    }
    Some(v)
}

fn term(t: &[&Tok], pos: &mut usize, n: &HashMap<String, f64>) -> Option<f64> {
    let mut v = atom(t, pos, n)?;
    while let Some(Tok::Punct(op @ ('*' | '/'))) = t.get(*pos) {
        *pos += 1;
        let r = atom(t, pos, n)?;
        v = if *op == '*' { v * r } else { v / r };
    }
    Some(v)
}

fn atom(t: &[&Tok], pos: &mut usize, n: &HashMap<String, f64>) -> Option<f64> {
    let tok = t.get(*pos)?;
    *pos += 1;
    match tok {
        Tok::Num(v, _) => Some(*v),
        Tok::Punct('-') => atom(t, pos, n).map(|v| -v),
        Tok::Punct('(') => {
            let v = expr(t, pos, n)?;
            (t.get(*pos) == Some(&&Tok::Punct(')'))).then(|| {
                *pos += 1;
                v
            })
        }
        Tok::Ident(name) => {
            // `Self::X`, `kit::X`: take the last path segment
            let mut name = name.clone();
            while t.get(*pos) == Some(&&Tok::Punct(':')) && t.get(*pos + 1) == Some(&&Tok::Punct(':')) {
                let Some(Tok::Ident(next)) = t.get(*pos + 2) else { return None };
                name = next.clone();
                *pos += 3;
            }
            n.get(&name).copied()
        }
        _ => None,
    }
}

/// Every reading of three numbers as an sRGB colour, given the name written before them.
fn readings(callee: &str, v: [f32; 3], ints: bool) -> Vec<[f32; 3]> {
    let lo = callee.to_ascii_lowercase();
    let mut out = Vec::new();
    let srgb = |c: Color| c.to_srgba().to_f32_array_no_alpha();
    let [a, b, c] = v;
    if lo.contains("hsl") {
        out.push(srgb(Hsla::hsl(a, b, c).into()));
    } else if lo.contains("hsv") {
        out.push(srgb(Hsva::hsv(a, b, c).into()));
    } else if lo.contains("hwb") {
        out.push(srgb(Hwba::hwb(a, b, c).into()));
    } else if lo.contains("oklch") {
        out.push(srgb(Oklcha::lch(a, b, c).into()));
    } else if lo.contains("oklab") {
        out.push(srgb(Oklaba::lab(a, b, c).into()));
    } else if lo.contains("lch") {
        out.push(srgb(Lcha::lch(a, b, c).into()));
    } else if lo.contains("lab") {
        out.push(srgb(Laba::lab(a, b, c).into()));
    } else if lo.contains("xyz") {
        out.push(srgb(Xyza::xyz(a, b, c).into()));
    } else if lo.contains("linear") {
        out.push(srgb(LinearRgba::rgb(a, b, c).into()));
    } else {
        if v.iter().all(|x| (0.0..=1.0).contains(x)) {
            out.push(v);
        }
        if (ints || v.iter().any(|x| *x > 1.0)) && v.iter().all(|x| (0.0..=255.0).contains(x)) {
            out.push(v.map(|x| x / 255.0));
        }
    }
    out
}

fn hex_colour(h: &str) -> Option<[f32; 3]> {
    let h = h.trim_start_matches('#');
    let full: String = match h.len() {
        3 | 4 => h.chars().take(3).flat_map(|c| [c, c]).collect(),
        6 | 8 => h[..6].to_string(),
        _ => return None,
    };
    Srgba::hex(&full).ok().map(|c| c.to_f32_array_no_alpha())
}

/// Cream colours written anywhere in `src`, with a short description of each.
pub fn cream_in(src: &str) -> Vec<String> {
    let toks = lex(src);
    let names = numeric_names(&toks);
    let mut found = Vec::new();
    let mut push = |what: String, c: [f32; 3]| {
        if is_cream(c) {
            found.push(format!("{what} = srgb({:.3}, {:.3}, {:.3})", c[0], c[1], c[2]));
        }
    };
    for (i, t) in toks.iter().enumerate() {
        match t {
            Tok::Str(s) => {
                // any 3/4/6/8-digit hex run inside a string, with or without '#'
                for word in s.split(|c: char| !c.is_ascii_hexdigit() && c != '#') {
                    let w = word.trim_start_matches('#');
                    let short = matches!(w.len(), 3 | 4) && word.starts_with('#');
                    if (matches!(w.len(), 6 | 8) || short)
                        && let Some(c) = hex_colour(w)
                    {
                        push(format!("\"{s}\""), c);
                    }
                }
            }
            Tok::Num(v, true) if *v >= 0x10_0000 as f64 => {
                let v = *v as u64;
                for rgb in [v & 0xFF_FFFF, v >> 8] {
                    if rgb <= 0xFF_FFFF {
                        let c = [(rgb >> 16) & 0xFF, (rgb >> 8) & 0xFF, rgb & 0xFF].map(|x| x as f32 / 255.0);
                        push(format!("0x{v:X}"), c);
                    }
                }
            }
            Tok::Punct(open @ ('(' | '[' | '{')) => {
                let close = match open {
                    '(' => ')',
                    '[' => ']',
                    _ => '}',
                };
                // split the group's top level on commas
                let mut depth = 0;
                let mut args: Vec<Vec<Tok>> = vec![Vec::new()];
                let mut j = i + 1;
                while j < toks.len() {
                    match &toks[j] {
                        Tok::Punct('(' | '[' | '{') => depth += 1,
                        Tok::Punct(c) if *c == close && depth == 0 => break,
                        Tok::Punct(')' | ']' | '}') => depth -= 1,
                        Tok::Punct(',') if depth == 0 => {
                            args.push(Vec::new());
                            j += 1;
                            continue;
                        }
                        _ => {}
                    }
                    args.last_mut().unwrap().push(toks[j].clone());
                    j += 1;
                    if args.len() > 4 {
                        break;
                    }
                }
                args.retain(|a| !a.is_empty());
                if !(3..=4).contains(&args.len()) {
                    continue;
                }
                // `red: 0.99` in struct literals: drop the field name
                let vals: Option<Vec<(f64, bool)>> = args
                    .iter()
                    .take(3)
                    .map(|a| {
                        let a: &[Tok] = if a.len() > 2 && matches!(a[0], Tok::Ident(_)) && a[1] == Tok::Punct(':') && a.get(2) != Some(&Tok::Punct(':')) { &a[2..] } else { a };
                        let int = a.iter().all(|t| !matches!(t, Tok::Num(_, false)));
                        eval(a, &names).map(|v| (v, int))
                    })
                    .collect();
                let Some(vals) = vals else { continue };
                let callee = match i.checked_sub(1).map(|k| &toks[k]) {
                    Some(Tok::Ident(n)) => n.clone(),
                    _ => String::new(),
                };
                let v = [vals[0].0 as f32, vals[1].0 as f32, vals[2].0 as f32];
                let ints = vals.iter().all(|(_, int)| *int);
                for c in readings(&callee, v, ints) {
                    push(format!("{callee}{open}{}, {}, {}", v[0], v[1], v[2]), c);
                }
            }
            _ => {}
        }
    }
    found
}

/// Places in `src` that set a corner radius: bevy UI's `BorderRadius` type or a node's
/// `border_radius` field, however the value is written.
pub fn radius_in(src: &str) -> Vec<String> {
    lex(src)
        .into_iter()
        .filter_map(|t| match t {
            Tok::Ident(n) if n == "BorderRadius" || n == "border_radius" => Some(n),
            _ => None,
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cream(src: &str) -> bool {
        !cream_in(src).is_empty()
    }

    #[test]
    fn finds_cream_however_it_is_written() {
        for src in [
            "Color::srgba(0.99, 0.95, 0.87, 0.95)",
            "Color::srgba(0.99,0.95,0.87,0.95)",
            "Color::srgb(  0.99 ,\n 0.95,0.87 )",
            "Color::srgb_u8(252, 242, 222)",
            "Color::srgba_u8(252,242,222,240)",
            "Srgba::hex(\"#fcf2de\")",
            "Srgba::hex(\"FCF2DE\").unwrap()",
            "let c = \"#FCF2DEF0\";",
            "const CREAM: u32 = 0xFCF2DE;",
            "const CREAM: u32 = 0xfcf2_deff;",
            "Srgba { red: 0.99, green: 0.95, blue: 0.87, alpha: 1.0 }",
            "Srgba::new(0.99f32, 0.95f32, 0.87f32, 1.)",
            "[0.99, 0.95, 0.87]",
            "Color::srgb(252.0 / 255.0, 242.0 / 255.0, 222.0 / 255.0)",
            "Color::srgb(252 as f32 / 255.0, 242 as f32 / 255.0, 222 as f32 / 255.0)",
            "const R: f32 = 0.99; const G: f32 = 0.95; const B: f32 = 0.87; fn f() -> Color { Color::srgb(R, G, B) }",
            "hex(0xFC, 0xF2, 0xDE)",
            "my_rgb(253, 243, 221)", // within a few bytes
            "Color::srgb(0.96, 0.89, 0.76)", // soft panel
            "Color::srgba(1.0, 0.98, 0.93, 0.9)", // row stripe
            "Color::linear_rgb(0.9772, 0.8900, 0.7293)",
            "LinearRgba { red: 0.9772, green: 0.8879, blue: 0.7305, alpha: 1.0 }",
            "Color::hsl(40.0, 0.834, 0.9294)",
        ] {
            assert!(cream(src), "missed cream in `{src}`");
        }
    }

    #[test]
    fn spec_colours_and_unrelated_numbers_are_not_cream() {
        for src in [
            "hex(0xF1, 0xE3, 0xC6)", // Pith, the spec's text colour
            "Color::srgb(0.30, 0.20, 0.13)",
            "Vec3::new(1.0, 0.0, 0.0)",
            "px(44)",
            "// Color::srgb(0.99, 0.95, 0.87) in a comment",
            "let x = 0xFFFFFF_u32 & mask;",
        ] {
            assert!(!cream(src), "false alarm in `{src}`: {:?}", cream_in(src));
        }
    }

    #[test]
    fn finds_any_radius_outside_the_kit() {
        for src in [
            "BorderRadius::MAX",
            "Node { border_radius: BorderRadius::all(Val::Px(99.0)), ..default() }",
            "n.border_radius = r;",
            "BorderRadius::all(Val::Percent(50.0))",
            "use bevy::ui::BorderRadius as R;",
        ] {
            assert!(!radius_in(src).is_empty(), "missed a radius in `{src}`");
        }
        assert!(radius_in("let radius = 4.0; // border_radius in a comment").is_empty());
    }
}
