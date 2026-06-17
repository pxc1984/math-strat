use crate::BestTable;
use crate::consts::MAX_TOTAL_SCORE;
use crate::error::{AppError, CacheError};
use crate::progress::ProgressTracker;
use colored::Color;
use std::io::ErrorKind;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::Ordering;
use std::{env, fs};

const CACHE_VERSION: &str = "v7";

fn cache_file_path() -> PathBuf {
    env::temp_dir()
        .join("math-strat")
        .join(format!("best-{CACHE_VERSION}.bin"))
}

pub fn load_best_from_cache(path: &PathBuf) -> Result<Option<BestTable>, CacheError> {
    let content = match fs::read(path) {
        Ok(content) => content,
        Err(error) if error.kind() == ErrorKind::NotFound => return Ok(None),
        Err(source) => {
            return Err(CacheError::Read {
                path: path.clone(),
                source,
            });
        }
    };

    let best: BestTable =
        bincode::deserialize(&content).map_err(|source| CacheError::Deserialize {
            path: path.clone(),
            source,
        })?;

    if best.len() == MAX_TOTAL_SCORE + 1 {
        Ok(Some(best))
    } else {
        Err(CacheError::InvalidSize {
            path: path.clone(),
            expected: MAX_TOTAL_SCORE + 1,
            actual: best.len(),
        })
    }
}

pub fn save_best_to_cache(path: &PathBuf, best: &BestTable) -> Result<(), CacheError> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|source| CacheError::CreateDir {
            path: parent.to_path_buf(),
            source,
        })?;
    }

    let content = bincode::serialize(best).map_err(CacheError::Serialize)?;
    fs::write(path, content).map_err(|source| CacheError::Write {
        path: path.clone(),
        source,
    })
}

fn warn_cache_error(error: &CacheError) -> Result<(), AppError> {
    crate::io::write_error_line(crate::io::status_line("warning:", Color::Yellow, error))?;
    Ok(())
}

pub fn load_or_compute_best() -> Result<BestTable, AppError> {
    let cache_path = cache_file_path();
    match load_best_from_cache(&cache_path) {
        Ok(Some(best)) => return Ok(best),
        Ok(None) => {}
        Err(error) => warn_cache_error(&error)?,
    }

    let progress = Arc::new(ProgressTracker::new(total_compute_iterations()));
    let reporter = crate::progress::start_progress_reporter(progress.clone())?;
    let best = crate::compute_best(progress.as_ref());
    progress.completed.store(progress.total, Ordering::Relaxed);
    progress.finished.store(true, Ordering::Relaxed);
    reporter
        .join()
        .map_err(|_| AppError::ThreadPanicked("progress reporter"))??;

    if let Err(error) = save_best_to_cache(&cache_path, &best) {
        warn_cache_error(&error)?;
    }

    Ok(best)
}

fn total_compute_iterations() -> u64 {
    crate::proof_config_count()
}
