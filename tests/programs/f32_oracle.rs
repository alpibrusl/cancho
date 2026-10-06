// `docs/f32.md` §5.3's oracle: what Rust's own formatting and parsing say about
// an `f32`, for `conformance/f32_text.rs` to hold `std.fmt32` against byte for
// byte. Compiled with `rustc -O` by the test that needs it; it reads stdin and
// writes stdout, and has no dependency.
//
//     f32_oracle debug               one `{:?}` per line, from decimal bit patterns
//     f32_oracle fixed <prec>        one `{:.prec$}` per line, from the same
//     f32_oracle parse               one result per line, from lines of text:
//                                    the bits as decimal, or `nan`, or `err`
//     f32_oracle sample <stride> <count>   a digest of `{:?}` over the sample set
//     f32_oracle range <lo> <hi>           a digest of `{:?}` over lo..hi
//
// A digest is `<lines> <fnv-1a-64 as i64>` over every line, each followed by
// `\n`, in order: the same two numbers `tests/programs/f32_exhaustive.cho`
// prints. The sample set (also built there, identically) is, for each sign and
// each exponent field 0..=255, the fractions `j * stride & 0x7fffff` for
// `j < count`, then `BOUNDARY`; NaNs are skipped.
use std::io::{BufRead, BufWriter, Write};

const BOUNDARY: [u32; 19] = [
    0, 1, 2, 3, 4, 5, 6, 7, 0x3fffff, 0x400000, 0x400001, 0x7ffff8, 0x7ffff9, 0x7ffffa, 0x7ffffb,
    0x7ffffc, 0x7ffffd, 0x7ffffe, 0x7fffff,
];

struct Digest {
    lines: u64,
    hash: u64,
}

impl Digest {
    fn new() -> Digest {
        Digest { lines: 0, hash: 0xcbf2_9ce4_8422_2325 }
    }
    fn add(&mut self, text: &str) {
        for byte in text.bytes().chain(std::iter::once(b'\n')) {
            self.hash = (self.hash ^ u64::from(byte)).wrapping_mul(0x0000_0100_0000_01b3);
        }
        self.lines += 1;
    }
    fn report(&self) {
        println!("{} {}", self.lines, self.hash as i64);
    }
}

fn is_nan_bits(bits: u32) -> bool {
    bits & 0x7fff_ffff > 0x7f80_0000
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let stdin = std::io::stdin();
    let mut out = BufWriter::new(std::io::stdout().lock());
    match args.get(1).map(String::as_str) {
        Some("debug") | Some("fixed") => {
            let prec: Option<usize> = args.get(2).map(|p| p.parse().expect("a precision"));
            for line in stdin.lock().lines() {
                let bits: u32 = line.expect("a line").trim().parse().expect("a bit pattern");
                let x = f32::from_bits(bits);
                match prec {
                    Some(p) => writeln!(out, "{x:.p$}"),
                    None => writeln!(out, "{x:?}"),
                }
                .expect("stdout");
            }
        }
        Some("parse") => {
            for line in stdin.lock().lines() {
                let line = line.expect("a line");
                match line.parse::<f32>() {
                    Ok(x) if x.is_nan() => writeln!(out, "nan"),
                    Ok(x) => writeln!(out, "{}", x.to_bits()),
                    Err(_) => writeln!(out, "err"),
                }
                .expect("stdout");
            }
        }
        Some("sample") => {
            let stride: u32 = args[2].parse().expect("a stride");
            let count: u32 = args[3].parse().expect("a count");
            let mut digest = Digest::new();
            for sign in 0..2u32 {
                for exponent in 0..256u32 {
                    let fractions = (0..count)
                        .map(|j| j.wrapping_mul(stride) & 0x7f_ffff)
                        .chain(BOUNDARY.iter().copied());
                    for fraction in fractions {
                        let bits = sign << 31 | exponent << 23 | fraction;
                        if !is_nan_bits(bits) {
                            digest.add(&format!("{:?}", f32::from_bits(bits)));
                        }
                    }
                }
            }
            digest.report();
        }
        Some("range") => {
            let lo: u64 = args[2].parse().expect("lo");
            let hi: u64 = args[3].parse().expect("hi");
            let mut digest = Digest::new();
            for bits in lo..hi {
                let bits = bits as u32;
                if !is_nan_bits(bits) {
                    digest.add(&format!("{:?}", f32::from_bits(bits)));
                }
            }
            digest.report();
        }
        _ => {
            eprintln!("usage: f32_oracle debug | fixed <prec> | parse | sample <stride> <count> | range <lo> <hi>");
            std::process::exit(2);
        }
    }
}
