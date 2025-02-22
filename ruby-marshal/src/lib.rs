mod convert;
mod dump;
mod load;
mod value_arena;

pub use self::convert::DisplayByteString;
pub use self::convert::FromValue;
pub use self::convert::FromValueContext;
pub use self::convert::FromValueError;
pub use self::convert::IntoValue;
pub use self::convert::IntoValueError;
pub use self::dump::dump;
pub use self::load::load;
pub use self::value_arena::ArrayValue;
pub use self::value_arena::BoolValue;
pub use self::value_arena::FixnumValue;
pub use self::value_arena::FloatValue;
pub use self::value_arena::HashValue;
pub use self::value_arena::NilValue;
pub use self::value_arena::ObjectValue;
pub use self::value_arena::StringValue;
pub use self::value_arena::SymbolValue;
pub use self::value_arena::TypedValueHandle;
pub use self::value_arena::UserDefinedValue;
pub use self::value_arena::ClassValue;
pub use self::value_arena::Value;
pub use self::value_arena::ValueArena;
pub use self::value_arena::ValueHandle;
pub use self::value_arena::ValueKind;

const MAJOR_VERSION: u8 = 4;
const MINOR_VERSION: u8 = 8;

const VALUE_KIND_NIL: u8 = b'0';
const VALUE_KIND_TRUE: u8 = b'T';
const VALUE_KIND_FALSE: u8 = b'F';
const VALUE_KIND_FIXNUM: u8 = b'i';
const VALUE_KIND_FLOAT: u8 = b'f';
const VALUE_KIND_SYMBOL: u8 = b':';
const VALUE_KIND_SYMBOL_LINK: u8 = b';';
const VALUE_KIND_OBJECT_LINK: u8 = b'@';
const VALUE_KIND_INSTANCE_VARIABLES: u8 = b'I';
const VALUE_KIND_ARRAY: u8 = b'[';
const VALUE_KIND_HASH: u8 = b'{';
const VALUE_KIND_HASH_DEFAULT: u8 = b'}';
const VALUE_KIND_OBJECT: u8 = b'o';
const VALUE_KIND_STRING: u8 = b'"';
const VALUE_KIND_USER_DEFINED: u8 = b'u';
const VALUE_KIND_CLASS: u8 = b'c';

/// The library error type
#[derive(Debug)]
pub enum Error {
    /// Invalid version
    InvalidVersion {
        /// The major version
        major: u8,

        /// The minor version
        minor: u8,
    },

    /// An I/O Error
    Io { error: std::io::Error },

    /// An invalid value kind was encountered
    InvalidValueKind { kind: u8 },

    /// A value handle was invalid
    InvalidValueHandle {
        /// The invalid value handle
        handle: ValueHandle,
    },

    /// The fixnum size is too large
    InvalidFixnumSize { size: u8 },

    /// The Fixnum is not a valid usize
    FixnumInvalidUSize { error: std::num::TryFromIntError },

    /// The usize is not a valid Fixnum
    USizeInvalidFixnum { error: std::num::TryFromIntError },

    /// Float string is cannot be decoded as utf-8
    InvalidFloatUtf8 { error: std::str::Utf8Error },

    /// Float cannot be parsed
    InvalidFloat { error: <f64 as std::str::FromStr>::Err },

    /// Missing a symbol link
    MissingSymbolLink { index: usize },

    /// Missing an object link
    MissingObjectLink { index: usize },

    /// Unexpected Value Kind
    UnexpectedValueKind { expected: u8, actual: u8 },

    /// The value is not an object
    NotAnObject,

    /// There was a duplicate instance variable
    DuplicateInstanceVariable {
        /// The duplicated variable
        name: Vec<u8>,
    },
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidVersion { major, minor } => write!(f, "invalid version {major}.{minor}"),
            Self::Io { .. } => write!(f, "I/O error"),
            Self::InvalidValueKind { kind } => write!(f, "invalid value kind {kind}"),
            Self::InvalidValueHandle { .. } => write!(f, "invalid value handle"),
            Self::InvalidFixnumSize { size } => write!(f, "invalid fixnum size {size}"),
            Self::FixnumInvalidUSize { .. } => write!(f, "fixnum is not a valid usize"),
            Self::USizeInvalidFixnum { .. } => write!(f, "usize is not a valid Fixnum"),
            Self::InvalidFloatUtf8 { .. } => write!(f, "float string is not valid utf-8"),
            Self::InvalidFloat { .. } => write!(f, "float cannot be parsed"),
            Self::MissingSymbolLink { index } => write!(f, "missing symbol link {index}"),
            Self::MissingObjectLink { index } => write!(f, "missing object link {index}"),
            Self::UnexpectedValueKind { expected, actual } => write!(
                f,
                "unexpected value kind, expected {expected} but got {actual}"
            ),
            Self::NotAnObject => write!(f, "not an object"),
            Self::DuplicateInstanceVariable { name } => {
                write!(f, "duplicate instance variable \"{name:?}\"")
            }
        }
    }
}

impl std::error::Error for Error {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Io { error } => Some(error),
            Self::FixnumInvalidUSize { error } => Some(error),
            Self::USizeInvalidFixnum { error } => Some(error),
            _ => None,
        }
    }
}

impl From<std::io::Error> for Error {
    fn from(error: std::io::Error) -> Self {
        Error::Io { error }
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use std::io::Read;

    #[test]
    fn kitchen_sink() {
        for entry in std::fs::read_dir("test_data").expect("failed to read \"test_data\"") {
            let entry = entry.expect("failed to read entry");
            let file_type = entry.file_type().expect("failed to get file type");
            if file_type.is_dir() {
                continue;
            }
            let entry_path = entry.path();

            let data = std::fs::read(&entry_path).expect("failed to read entry");
            let mut data_reader = std::io::Cursor::new(&data);

            println!("entry_path = {:?}", entry_path);
            let value_arena = load(&mut data_reader).expect("failed to load");

            let mut new_data = Vec::new();
            dump(&mut new_data, &value_arena).expect("failed to dump");

            let read_end_result = data_reader.read(&mut [0]);
            let is_eof = matches!(read_end_result, Ok(0));
            assert!(is_eof);
            assert!(data == new_data, "{data:?} != {new_data:?}");
        }
    }
}
