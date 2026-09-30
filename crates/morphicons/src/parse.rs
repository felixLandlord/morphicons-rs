//! Parser for SVG path data (the `d` attribute). Resolves relative commands
//! and shorthands (H/V → L, S/T → C/Q with control-point reflection) and
//! returns raw subpaths with absolute coordinates. Supports implicit
//! repetition, extra pairs after M (implicit lineto), packed arc flags and
//! scientific notation. Conversion to cubics lives in `normalize`.

use crate::{Error, Point};

/// Raw absolute segment, shorthands already resolved.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum RawSeg {
    Line(Point),
    Cubic(Point, Point, Point),
    Quad(Point, Point),
    Arc {
        rx: f64,
        ry: f64,
        rotation: f64,
        large: bool,
        sweep: bool,
        to: Point,
    },
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct RawSubpath {
    pub start: Point,
    pub segs: Vec<RawSeg>,
    pub closed: bool,
}

const COMMANDS: &[u8] = b"MmLlHhVvCcSsQqTtAaZz";

struct Cursor<'a> {
    s: &'a [u8],
    i: usize,
}

impl Cursor<'_> {
    fn err(&self, message: impl Into<String>) -> Error {
        Error::Parse {
            message: message.into(),
            offset: self.i,
        }
    }

    fn peek(&self) -> Option<u8> {
        self.s.get(self.i).copied()
    }

    fn skip(&mut self) {
        while matches!(
            self.peek(),
            Some(b' ' | b'\t' | b'\n' | b'\r' | 0x0c | b',')
        ) {
            self.i += 1;
        }
    }

    fn digits(&mut self) -> bool {
        let start = self.i;
        while matches!(self.peek(), Some(b'0'..=b'9')) {
            self.i += 1;
        }
        self.i > start
    }

    fn num(&mut self) -> Result<f64, Error> {
        self.skip();
        let start = self.i;
        if matches!(self.peek(), Some(b'+' | b'-')) {
            self.i += 1;
        }
        let mut dig = self.digits();
        if self.peek() == Some(b'.') {
            self.i += 1;
            dig |= self.digits();
        }
        if !dig {
            return Err(self.err("expected number"));
        }
        if matches!(self.peek(), Some(b'e' | b'E')) {
            let save = self.i;
            self.i += 1;
            if matches!(self.peek(), Some(b'+' | b'-')) {
                self.i += 1;
            }
            if !self.digits() {
                self.i = save; // dangling "e": not an exponent
            }
        }
        // The scanned slice is pure ASCII by construction.
        std::str::from_utf8(&self.s[start..self.i])
            .ok()
            .and_then(|t| t.parse::<f64>().ok())
            .ok_or_else(|| self.err("invalid number"))
    }

    /// Arc flag: a single 0|1 character; accepts the packed form "011 1".
    fn flag(&mut self) -> Result<bool, Error> {
        self.skip();
        match self.peek() {
            Some(b'0') => {
                self.i += 1;
                Ok(false)
            }
            Some(b'1') => {
                self.i += 1;
                Ok(true)
            }
            _ => Err(self.err("expected arc flag (0|1)")),
        }
    }
}

/// The subpath in progress; after Z, a drawing command opens a new one at the
/// current point (which Z moved back to the subpath start).
fn open<'a>(
    subs: &'a mut Vec<RawSubpath>,
    cur: &mut Option<usize>,
    started: bool,
    at: Point,
    cursor: &Cursor,
) -> Result<&'a mut RawSubpath, Error> {
    if !started {
        return Err(cursor.err("path must start with M/m"));
    }
    let idx = match *cur {
        Some(idx) => idx,
        None => {
            subs.push(RawSubpath {
                start: at,
                segs: Vec::new(),
                closed: false,
            });
            *cur = Some(subs.len() - 1);
            subs.len() - 1
        }
    };
    Ok(&mut subs[idx])
}

#[derive(Clone, Copy, PartialEq)]
enum Prev {
    None,
    Cubic,
    Quad,
}

pub(crate) fn parse_path(d: &str) -> Result<Vec<RawSubpath>, Error> {
    let mut c = Cursor {
        s: d.as_bytes(),
        i: 0,
    };
    let mut subs: Vec<RawSubpath> = Vec::new();
    let mut cur: Option<usize> = None;
    let mut pos = Point::ZERO; // current point
    let mut start = Point::ZERO; // start of the current subpath
    let mut ctrl = Point::ZERO; // last control point (S/T reflection)
    let mut prev = Prev::None;
    let mut started = false; // the first command must be M/m
    let mut cmd: u8 = 0;

    loop {
        c.skip();
        let Some(ch) = c.peek() else { break };
        if COMMANDS.contains(&ch) {
            cmd = ch;
            c.i += 1;
        } else if cmd == 0 {
            return Err(c.err("path must start with M/m"));
        } else if cmd == b'M' {
            cmd = b'L'; // extra pairs after M → implicit lineto
        } else if cmd == b'm' {
            cmd = b'l';
        } else if cmd == b'Z' || cmd == b'z' {
            return Err(c.err("stray data after Z"));
        }

        // Relative coordinates add the current point, which is not updated
        // until the command finishes: all pairs of a relative C/S/Q share it.
        let rel = cmd.is_ascii_lowercase();
        let origin = if rel { pos } else { Point::ZERO };
        let pt = |c: &mut Cursor| -> Result<Point, Error> {
            let x = c.num()? + origin.x;
            let y = c.num()? + origin.y;
            Ok(Point::new(x, y))
        };

        match cmd.to_ascii_uppercase() {
            b'M' => {
                started = true;
                let p = pt(&mut c)?;
                pos = p;
                start = p;
                subs.push(RawSubpath {
                    start: p,
                    segs: Vec::new(),
                    closed: false,
                });
                cur = Some(subs.len() - 1);
                prev = Prev::None;
            }
            b'L' => {
                let p = pt(&mut c)?;
                open(&mut subs, &mut cur, started, pos, &c)?
                    .segs
                    .push(RawSeg::Line(p));
                pos = p;
                prev = Prev::None;
            }
            b'H' => {
                let x = c.num()? + origin.x;
                let p = Point::new(x, pos.y);
                open(&mut subs, &mut cur, started, pos, &c)?
                    .segs
                    .push(RawSeg::Line(p));
                pos = p;
                prev = Prev::None;
            }
            b'V' => {
                let y = c.num()? + origin.y;
                let p = Point::new(pos.x, y);
                open(&mut subs, &mut cur, started, pos, &c)?
                    .segs
                    .push(RawSeg::Line(p));
                pos = p;
                prev = Prev::None;
            }
            b'C' | b'S' => {
                let c1 = if cmd == b'C' || cmd == b'c' {
                    pt(&mut c)?
                } else if prev == Prev::Cubic {
                    pos * 2.0 - ctrl // reflect the previous control point
                } else {
                    pos
                };
                let c2 = pt(&mut c)?;
                let p = pt(&mut c)?;
                open(&mut subs, &mut cur, started, pos, &c)?
                    .segs
                    .push(RawSeg::Cubic(c1, c2, p));
                ctrl = c2;
                pos = p;
                prev = Prev::Cubic;
            }
            b'Q' | b'T' => {
                let q = if cmd == b'Q' || cmd == b'q' {
                    pt(&mut c)?
                } else if prev == Prev::Quad {
                    pos * 2.0 - ctrl
                } else {
                    pos
                };
                let p = pt(&mut c)?;
                open(&mut subs, &mut cur, started, pos, &c)?
                    .segs
                    .push(RawSeg::Quad(q, p));
                ctrl = q;
                pos = p;
                prev = Prev::Quad;
            }
            b'A' => {
                let rx = c.num()?;
                let ry = c.num()?;
                let rotation = c.num()?;
                let large = c.flag()?;
                let sweep = c.flag()?;
                let to = pt(&mut c)?;
                open(&mut subs, &mut cur, started, pos, &c)?
                    .segs
                    .push(RawSeg::Arc {
                        rx,
                        ry,
                        rotation,
                        large,
                        sweep,
                        to,
                    });
                pos = to;
                prev = Prev::None;
            }
            b'Z' => {
                if let Some(idx) = cur.take() {
                    subs[idx].closed = true;
                }
                pos = start;
                prev = Prev::None;
            }
            _ => unreachable!("COMMANDS only holds known letters"),
        }
    }

    subs.retain(|s| !s.segs.is_empty());
    Ok(subs)
}
