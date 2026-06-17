use crate::BestTable;
use crate::consts::MAX_TOTAL_SCORE;
use crate::progress::ProgressTracker;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::Ordering;
use std::{env, fs};

const CACHE_VERSION: &str = "v6";

fn cache_file_path() -> PathBuf {
    env::temp_dir()
        .join("math-strat")
        .join(format!("best-{CACHE_VERSION}.json"))
}

pub fn load_best_from_cache(path: &PathBuf) -> Option<BestTable> {
    let content = fs::read_to_string(path).ok()?;
    let best: BestTable = serde_json::from_str(&content).ok()?;
    if best.len() == MAX_TOTAL_SCORE + 1 {
        Some(best)
    } else {
        None
    }
}

pub fn save_best_to_cache(path: &PathBuf, best: &BestTable) {
    if let Some(parent) = path.parent() {
        let _ = fs::create_dir_all(parent);
    }

    if let Ok(content) = serde_json::to_string(best) {
        let _ = fs::write(path, content);
    }
}

pub fn load_or_compute_best() -> BestTable {
    let cache_path = cache_file_path();
    if let Some(best) = load_best_from_cache(&cache_path) {
        return best;
    }

    let progress = Arc::new(ProgressTracker::new(total_compute_iterations()));
    let reporter = crate::progress::start_progress_reporter(progress.clone());
    let best = crate::compute_best(progress.as_ref());
    progress.completed.store(progress.total, Ordering::Relaxed);
    progress.finished.store(true, Ordering::Relaxed);
    let _ = reporter.join();
    save_best_to_cache(&cache_path, &best);
    best
}

fn total_compute_iterations() -> u64 {
    crate::proof_config_count()
}
