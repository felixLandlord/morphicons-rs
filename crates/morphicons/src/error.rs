use std::fmt;

/// Errors raised while turning icon data into geometry. Everything after
/// parsing (planning, interpolation, animation) is infallible.
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub enum Error {
    /// Malformed SVG path data. `offset` is the byte index in the `d` string.
    Parse { message: String, offset: usize },
    /// An icon node tag outside the seven stroke primitives
    /// (`path`, `line`, `circle`, `ellipse`, `rect`, `polyline`, `polygon`).
    UnsupportedTag(String),
    /// A `points` attribute that isn't a list of numbers.
    InvalidPoints(String),
    /// A view box that isn't four finite numbers with positive size.
    InvalidViewBox(String),
    /// The icon produced no drawable subpaths.
    Empty,
    /// A subpath has more corners than the sample count can anchor.
    TooManyCorners { runs: usize, samples: usize },
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::Parse { message, offset } => write!(f, "morphicons: {message} at d[{offset}]"),
            Error::UnsupportedTag(tag) => write!(f, "morphicons: unsupported tag <{tag}>"),
            Error::InvalidPoints(s) => write!(f, "morphicons: invalid points: \"{s}\""),
            Error::InvalidViewBox(s) => write!(f, "morphicons: invalid viewBox \"{s}\""),
            Error::Empty => write!(f, "morphicons: icon has no subpaths"),
            Error::TooManyCorners { runs, samples } => {
                write!(f, "morphicons: N={samples} too small ({runs} runs)")
            }
        }
    }
}

impl std::error::Error for Error {}
