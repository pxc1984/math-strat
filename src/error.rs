use std::fmt;
use std::io;
use std::path::PathBuf;

#[derive(Debug)]
pub enum AppError {
    Io(io::Error),
    Cache(CacheError),
    ThreadPanicked(&'static str),
}

impl fmt::Display for AppError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io(error) => write!(f, "ошибка ввода-вывода: {error}"),
            Self::Cache(error) => write!(f, "{error}"),
            Self::ThreadPanicked(name) => write!(f, "внутренний поток '{name}' завершился с panic"),
        }
    }
}

impl std::error::Error for AppError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Io(error) => Some(error),
            Self::Cache(error) => Some(error),
            Self::ThreadPanicked(_) => None,
        }
    }
}

impl From<io::Error> for AppError {
    fn from(error: io::Error) -> Self {
        Self::Io(error)
    }
}

impl From<CacheError> for AppError {
    fn from(error: CacheError) -> Self {
        Self::Cache(error)
    }
}

#[derive(Debug)]
pub enum CacheError {
    Read {
        path: PathBuf,
        source: io::Error,
    },
    Deserialize {
        path: PathBuf,
        source: bincode::Error,
    },
    InvalidSize {
        path: PathBuf,
        expected: usize,
        actual: usize,
    },
    CreateDir {
        path: PathBuf,
        source: io::Error,
    },
    Serialize(bincode::Error),
    Write {
        path: PathBuf,
        source: io::Error,
    },
}

impl fmt::Display for CacheError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Read { path, source } => {
                write!(f, "не удалось прочитать кэш '{}': {source}", path.display())
            }
            Self::Deserialize { path, source } => write!(
                f,
                "не удалось декодировать кэш '{}': {source}",
                path.display()
            ),
            Self::InvalidSize {
                path,
                expected,
                actual,
            } => write!(
                f,
                "кэш '{}' имеет неверный размер: ожидалось {expected}, получено {actual}",
                path.display()
            ),
            Self::CreateDir { path, source } => write!(
                f,
                "не удалось создать директорию кэша '{}': {source}",
                path.display()
            ),
            Self::Serialize(source) => write!(f, "не удалось сериализовать кэш: {source}"),
            Self::Write { path, source } => {
                write!(f, "не удалось записать кэш '{}': {source}", path.display())
            }
        }
    }
}

impl std::error::Error for CacheError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Read { source, .. } => Some(source),
            Self::Deserialize { source, .. } => Some(source.as_ref()),
            Self::InvalidSize { .. } => None,
            Self::CreateDir { source, .. } => Some(source),
            Self::Serialize(source) => Some(source.as_ref()),
            Self::Write { source, .. } => Some(source),
        }
    }
}
