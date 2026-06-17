use crate::format;
use colored::Colorize;
use std::io::Write;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::thread;
use std::time::{Duration, Instant};

fn render_progress_line(progress: &ProgressTracker) -> String {
    let completed = progress
        .completed
        .load(Ordering::Relaxed)
        .min(progress.total);
    let ratio = if progress.total == 0 {
        1.0
    } else {
        completed as f64 / progress.total as f64
    };
    let filled = (ratio * PROGRESS_BAR_WIDTH as f64).round() as usize;
    let elapsed_secs = progress.started_at.elapsed().as_secs_f64().max(0.001);
    let speed = completed as f64 / elapsed_secs;
    let remaining = progress.total.saturating_sub(completed);
    let eta = if completed == 0 {
        "--".to_string()
    } else {
        format::format_duration(Duration::from_secs_f64(remaining as f64 / speed.max(0.001)))
    };

    let ratio_percent = ratio * 100.0;
    let bar = format!(
        "{:<width$}",
        "#".repeat(filled.min(PROGRESS_BAR_WIDTH)),
        width = PROGRESS_BAR_WIDTH
    );
    let colored_bar = if ratio_percent >= 100.0 {
        bar.green().to_string()
    } else if ratio_percent >= 50.0 {
        bar.yellow().to_string()
    } else {
        bar.cyan().to_string()
    };

    format!(
        "\r{}: [{}] {} | {} | {} | {} {}",
        "Прогресс".bold(),
        colored_bar,
        format!("{ratio_percent:>5.1}%").blue(),
        format!("{completed}/{}", progress.total).dimmed(),
        format!("{speed:>8.0} ит/с").magenta(),
        "осталось".dimmed(),
        eta,
    )
}

pub fn start_progress_reporter(progress: Arc<ProgressTracker>) -> thread::JoinHandle<()> {
    print!("{}", render_progress_line(&progress));
    let _ = std::io::stdout().flush();

    thread::spawn(move || {
        while !progress.finished.load(Ordering::Relaxed) {
            thread::sleep(Duration::from_millis(100));
            print!("{}", render_progress_line(&progress));
            let _ = std::io::stdout().flush();
        }

        print!("{}", render_progress_line(&progress));
        println!();
    })
}

const PROGRESS_BAR_WIDTH: usize = 10;

pub struct ProgressTracker {
    pub total: u64,
    pub completed: AtomicU64,
    pub finished: AtomicBool,
    pub started_at: Instant,
}

impl ProgressTracker {
    pub fn new(total: u64) -> Self {
        Self {
            total,
            completed: AtomicU64::new(0),
            finished: AtomicBool::new(false),
            started_at: Instant::now(),
        }
    }
}
