use core::fmt;

pub type Result<T> = core::result::Result<T, ParseError>;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParseError {
    pub kind: ErrorKind,
    pub offset: Option<usize>,
}

impl ParseError {
    pub const fn new(kind: ErrorKind) -> Self {
        Self { kind, offset: None }
    }

    pub const fn at(kind: ErrorKind, offset: usize) -> Self {
        Self {
            kind,
            offset: Some(offset),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ErrorKind {
    InvalidUtf8,
    UnexpectedEof(&'static str),
    MissingField(&'static str),
    InvalidField(&'static str),
    InvalidControl(&'static str),
    InvalidXml(&'static str),
    Oversized(&'static str),
    Mismatch(&'static str),
}

impl fmt::Display for ParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.offset {
            Some(offset) => write!(f, "{} at byte {}", self.kind, offset),
            None => write!(f, "{}", self.kind),
        }
    }
}

impl fmt::Display for ErrorKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidUtf8 => f.write_str("input is not valid UTF-8"),
            Self::UnexpectedEof(ctx) => write!(f, "unexpected end of input while parsing {ctx}"),
            Self::MissingField(field) => write!(f, "missing required field {field}"),
            Self::InvalidField(field) => write!(f, "invalid field {field}"),
            Self::InvalidControl(ctrl) => write!(f, "invalid control structure {ctrl}"),
            Self::InvalidXml(ctx) => write!(f, "invalid XML: {ctx}"),
            Self::Oversized(ctx) => write!(f, "oversized payload: {ctx}"),
            Self::Mismatch(field) => write!(f, "payload does not match bulletin field {field}"),
        }
    }
}

impl std::error::Error for ParseError {}
