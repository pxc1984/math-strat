use colored::{Color, ColoredString, Colorize};
use rayon::prelude::*;
use serde::{Deserialize, Serialize};
use std::env;
use std::fs;
use std::io::{self, Write};
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::thread;
use std::time::{Duration, Instant};

const DEFS_COSTS: [u32; 24] = [
    1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 2, 2, 2, 2, 2, 2, 2, 3, 3,
];
const RED_PROOFS_FORMS_COSTS: [f64; 21] = [
    1.0, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0,
    1.0, 1.0,
];
const BLACK_PROOFS_FORMS_COSTS: [f64; 12] = [2.0, 2.0, 2.0, 2.0, 2.0, 2.0, 2.0, 2.0, 2.0, 2.0, 2.0, 2.0];
const RED_PROOFS_BODY_COSTS: [u32; 21] = [
    2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2,
];
const BLACK_PROOFS_BODY_COSTS: [u32; 12] = [3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3];

const TOTAL_DEF_QUESTIONS: u32 = 3;
const TOTAL_FORM_QUESTIONS: u32 = 2;
const TOTAL_RED_PROOF_QUESTIONS: u32 = 3;
const TOTAL_BLACK_PROOF_QUESTIONS: u32 = 1;

const DEFINITION_QUESTION_SCORE: usize = 1;
const FORMULATION_QUESTION_SCORE: usize = 1;
const RED_PROOF_FORMULATION_SCORE: usize = 1;
const RED_PROOF_BODY_SCORE: usize = 2;
const BLACK_PROOF_FORMULATION_SCORE: usize = 1;
const BLACK_PROOF_BODY_SCORE: usize = 3;

const MAX_TOTAL_SCORE: usize = TOTAL_DEF_QUESTIONS as usize * DEFINITION_QUESTION_SCORE
    + TOTAL_FORM_QUESTIONS as usize * FORMULATION_QUESTION_SCORE
    + TOTAL_RED_PROOF_QUESTIONS as usize * (RED_PROOF_FORMULATION_SCORE + RED_PROOF_BODY_SCORE)
    + TOTAL_BLACK_PROOF_QUESTIONS as usize * (BLACK_PROOF_FORMULATION_SCORE + BLACK_PROOF_BODY_SCORE);
const TARGET_SCORE_COUNT: usize = MAX_TOTAL_SCORE + 1;
const TOTAL_DEF_CARDS: u32 = 24;
const TOTAL_FORM_CARDS: u32 = 56;
const TOTAL_RED_PROOF_CARDS: u32 = 21;
const TOTAL_BLACK_PROOF_CARDS: u32 = 12;
const TOTAL_PROOF_QUESTIONS: u32 = TOTAL_RED_PROOF_QUESTIONS + TOTAL_BLACK_PROOF_QUESTIONS;
const TOTAL_FORM_CARDS_AFTER_PROOFS: u32 = TOTAL_FORM_CARDS - TOTAL_PROOF_QUESTIONS;
const MAX_PROOF_FORM_CARDS: usize = (TOTAL_RED_PROOF_CARDS + TOTAL_BLACK_PROOF_CARDS) as usize;
const MAX_DRAWN_PROOF_FORMS: usize = 4;

type Outcomes = Vec<(u8, f64)>;
type Distribution = [f64; MAX_TOTAL_SCORE + 1];
type DefPmfTable = Vec<Outcomes>;
type FormPmfTable = Vec<Vec<Outcomes>>;

const CACHE_VERSION: &str = "v6";
const PROGRESS_BAR_WIDTH: usize = 10;

#[derive(Clone, Copy, Serialize, Deserialize)]
struct BestEntry {
    cost: f64,
    k_def: u8,
    k_red_pf: u8,
    k_black_pf: u8,
    k_red_pp: u8,
    k_black_pp: u8,
}

type BestTable = Vec<Option<BestEntry>>;

struct ProgressTracker {
    total: u64,
    completed: AtomicU64,
    finished: AtomicBool,
    started_at: Instant,
}

#[derive(Clone)]
struct ProofOutcome {
    score: usize,
    prob: f64,
    drawn_pf_known: u8,
}

#[derive(Clone)]
struct ProofConfig {
    proof_cost: f64,
    total_pf: u8,
    k_red_pf: u8,
    k_black_pf: u8,
    k_red_pp: u8,
    k_black_pp: u8,
    outcomes: Vec<ProofOutcome>,
}

fn label(text: &str, color: Color) -> ColoredString {
    text.color(color).bold()
}

fn status_line(text: &str, color: Color, message: impl std::fmt::Display) -> String {
    format!("{:>12} {}", label(text, color), message)
}

fn field_line(text: &str, color: Color, value: impl std::fmt::Display) -> String {
    format!("{:>12} {}", label(text, color), value)
}

fn format_count(chosen: u8, total: u32) -> String {
    format!("{}/{} шт", chosen.to_string().bold(), total)
}

fn comb(n: u32, k: u32) -> f64 {
    if k > n {
        return 0.0;
    }
    let k = k.min(n - k);
    if k == 0 {
        return 1.0;
    }

    let mut result = 1.0;
    for i in 0..k {
        result *= (n - i) as f64;
        result /= (i + 1) as f64;
    }
    result
}

fn prefix_sums_u32(costs: &[u32]) -> Vec<f64> {
    let mut prefix = Vec::with_capacity(costs.len() + 1);
    prefix.push(0.0);
    let mut acc = 0.0;
    for &cost in costs {
        acc += cost as f64;
        prefix.push(acc);
    }
    prefix
}

fn prefix_sums_f64(costs: &[f64]) -> Vec<f64> {
    let mut prefix = Vec::with_capacity(costs.len() + 1);
    prefix.push(0.0);
    let mut acc = 0.0;
    for &cost in costs {
        acc += cost;
        prefix.push(acc);
    }
    prefix
}

fn build_def_pmf_table() -> DefPmfTable {
    (0..=DEFS_COSTS.len() as u8).map(build_def_pmf).collect()
}

fn build_def_pmf(k_def: u8) -> Outcomes {
    let k_def = k_def as u32;
    let total = comb(TOTAL_DEF_CARDS, TOTAL_DEF_QUESTIONS);
    let mut outcomes = Vec::with_capacity(TOTAL_DEF_QUESTIONS as usize + 1);

    for x in 0..=TOTAL_DEF_QUESTIONS {
        if k_def < x || TOTAL_DEF_CARDS - k_def < TOTAL_DEF_QUESTIONS - x {
            continue;
        }

        let prob = comb(k_def, x) * comb(TOTAL_DEF_CARDS - k_def, TOTAL_DEF_QUESTIONS - x) / total;
        if prob > 0.0 {
            outcomes.push((x as u8, prob));
        }
    }

    outcomes
}

fn build_form_pmf_table() -> FormPmfTable {
    (0..=MAX_PROOF_FORM_CARDS)
        .map(|k_pf| {
            (0..=MAX_DRAWN_PROOF_FORMS)
                .map(|drawn_pf_known| build_form_pmf(k_pf as u8, drawn_pf_known as u8))
                .collect()
        })
        .collect()
}

fn build_form_pmf(k_pf: u8, drawn_pf_known: u8) -> Outcomes {
    let k_pf = k_pf as u32;
    let drawn_pf_known = drawn_pf_known as u32;
    let known_remaining = k_pf.saturating_sub(drawn_pf_known);

    if known_remaining > TOTAL_FORM_CARDS_AFTER_PROOFS {
        return Vec::new();
    }

    if known_remaining == 0 {
        return vec![(0, 1.0)];
    }

    let total_ways = comb(TOTAL_FORM_CARDS_AFTER_PROOFS, TOTAL_FORM_QUESTIONS);
    let mut outcomes = Vec::with_capacity(TOTAL_FORM_QUESTIONS as usize + 1);

    for x in 0..=TOTAL_FORM_QUESTIONS {
        if known_remaining < x || TOTAL_FORM_CARDS_AFTER_PROOFS - known_remaining < TOTAL_FORM_QUESTIONS - x {
            continue;
        }

        let prob = comb(known_remaining, x)
            * comb(TOTAL_FORM_CARDS_AFTER_PROOFS - known_remaining, TOTAL_FORM_QUESTIONS - x)
            / total_ways;
        if prob > 0.0 {
            outcomes.push((x as u8, prob));
        }
    }

    outcomes
}

fn form_pmf(table: &FormPmfTable, k_pf: u8, drawn_pf_known: u8) -> &Outcomes {
    &table[k_pf as usize][drawn_pf_known as usize]
}

fn build_proof_outcomes(
    k_red_pf: u8,
    k_black_pf: u8,
    k_red_pp: u8,
    k_black_pp: u8,
) -> Vec<ProofOutcome> {
    let total_red_proof_ways = comb(TOTAL_RED_PROOF_CARDS, TOTAL_RED_PROOF_QUESTIONS);
    let black_total = TOTAL_BLACK_PROOF_CARDS as f64;
    let mut outcomes = Vec::with_capacity(16);

    for red_full in 0..=TOTAL_RED_PROOF_QUESTIONS {
        if red_full > k_red_pp as u32 {
            break;
        }

        let max_red_form_only = (TOTAL_RED_PROOF_QUESTIONS - red_full).min((k_red_pf - k_red_pp) as u32);
        for red_form_only in 0..=max_red_form_only {
            let red_unknown = TOTAL_RED_PROOF_QUESTIONS - red_full - red_form_only;
            if red_unknown > TOTAL_RED_PROOF_CARDS - k_red_pf as u32 {
                continue;
            }

            let red_prob = comb(k_red_pp as u32, red_full)
                * comb((k_red_pf - k_red_pp) as u32, red_form_only)
                * comb(TOTAL_RED_PROOF_CARDS - k_red_pf as u32, red_unknown)
                / total_red_proof_ways;
            if red_prob == 0.0 {
                continue;
            }

            let red_score = red_full as usize
                * (RED_PROOF_FORMULATION_SCORE + RED_PROOF_BODY_SCORE)
                + red_form_only as usize * RED_PROOF_FORMULATION_SCORE;

            if k_black_pp > 0 {
                let black_prob = k_black_pp as f64 / black_total;
                outcomes.push(ProofOutcome {
                    score: red_score + BLACK_PROOF_FORMULATION_SCORE + BLACK_PROOF_BODY_SCORE,
                    prob: red_prob * black_prob,
                    drawn_pf_known: (red_full + red_form_only + 1) as u8,
                });
            }

            let black_form_only_count = k_black_pf.saturating_sub(k_black_pp);
            if black_form_only_count > 0 {
                let black_prob = black_form_only_count as f64 / black_total;
                outcomes.push(ProofOutcome {
                    score: red_score + BLACK_PROOF_FORMULATION_SCORE,
                    prob: red_prob * black_prob,
                    drawn_pf_known: (red_full + red_form_only + 1) as u8,
                });
            }

            let black_unknown_count = TOTAL_BLACK_PROOF_CARDS as i32 - k_black_pf as i32;
            if black_unknown_count > 0 {
                let black_prob = black_unknown_count as f64 / black_total;
                outcomes.push(ProofOutcome {
                    score: red_score,
                    prob: red_prob * black_prob,
                    drawn_pf_known: (red_full + red_form_only) as u8,
                });
            }
        }
    }

    outcomes
}

fn score_for_config(
    def_outcomes: &Outcomes,
    form_pmf_table: &FormPmfTable,
    proof_config: &ProofConfig,
) -> usize {
    let mut distribution = [0.0; MAX_TOTAL_SCORE + 1];

    for outcome in &proof_config.outcomes {
        let form_outcomes = form_pmf(form_pmf_table, proof_config.total_pf, outcome.drawn_pf_known);
        accumulate_scores(
            &mut distribution,
            outcome.score,
            outcome.prob,
            form_outcomes,
            def_outcomes,
        );
    }

    check_score(&distribution)
}

fn accumulate_scores(
    distribution: &mut Distribution,
    proof_score: usize,
    proof_prob: f64,
    form_outcomes: &Outcomes,
    def_outcomes: &Outcomes,
) {
    if proof_prob == 0.0 {
        return;
    }

    for &(form_score, form_prob) in form_outcomes {
        let partial_prob = proof_prob * form_prob;
        if partial_prob == 0.0 {
            continue;
        }

        for &(def_score, def_prob) in def_outcomes {
            let total_prob = partial_prob * def_prob;
            if total_prob == 0.0 {
                continue;
            }

            let total_score = proof_score + form_score as usize + def_score as usize;
            distribution[total_score] += total_prob;
        }
    }
}

fn check_score(distribution: &Distribution) -> usize {
    let mut cumulative = 0.0;
    for score in (0..=MAX_TOTAL_SCORE).rev() {
        cumulative += distribution[score];
        if cumulative >= 0.9 {
            return score;
        }
    }
    0
}

fn build_proof_configs(
    red_proof_form_cost_prefix: &[f64],
    black_proof_form_cost_prefix: &[f64],
    red_proof_body_cost_prefix: &[f64],
    black_proof_body_cost_prefix: &[f64],
) -> Vec<ProofConfig> {
    let mut configs = Vec::new();

    for k_red_pf in 0..=TOTAL_RED_PROOF_CARDS as usize {
        for k_black_pf in 0..=TOTAL_BLACK_PROOF_CARDS as usize {
            let total_pf = k_red_pf + k_black_pf;
            let proof_form_cost = red_proof_form_cost_prefix[k_red_pf] + black_proof_form_cost_prefix[k_black_pf];
            for k_red_pp in 0..=k_red_pf {
                for k_black_pp in 0..=k_black_pf {
                    let proof_body_cost = red_proof_body_cost_prefix[k_red_pp] + black_proof_body_cost_prefix[k_black_pp];
                    configs.push(ProofConfig {
                        proof_cost: proof_form_cost + proof_body_cost,
                        total_pf: total_pf as u8,
                        k_red_pf: k_red_pf as u8,
                        k_black_pf: k_black_pf as u8,
                        k_red_pp: k_red_pp as u8,
                        k_black_pp: k_black_pp as u8,
                        outcomes: build_proof_outcomes(
                            k_red_pf as u8,
                            k_black_pf as u8,
                            k_red_pp as u8,
                            k_black_pp as u8,
                        ),
                    });
                }
            }
        }
    }

    configs
}

fn proof_config_count() -> u64 {
    (0..=TOTAL_RED_PROOF_CARDS as u64)
        .map(|k_red_pf| {
            (0..=TOTAL_BLACK_PROOF_CARDS as u64)
                .map(|k_black_pf| (k_red_pf + 1) * (k_black_pf + 1))
                .sum::<u64>()
        })
        .sum()
}

fn total_compute_iterations() -> u64 {
    proof_config_count()
}

fn update_best_for_config(
    best: &mut BestTable,
    def_cost: f64,
    def_outcomes: &Outcomes,
    form_pmf_table: &FormPmfTable,
    k_def: u8,
    proof_config: &ProofConfig,
) {
    let max_score = score_for_config(def_outcomes, form_pmf_table, proof_config);
    let total_cost = def_cost + proof_config.proof_cost;

    for target_score in 0..=max_score {
        let candidate = BestEntry {
            cost: total_cost,
            k_def,
            k_red_pf: proof_config.k_red_pf,
            k_black_pf: proof_config.k_black_pf,
            k_red_pp: proof_config.k_red_pp,
            k_black_pp: proof_config.k_black_pp,
        };

        match best[target_score] {
            Some(entry) if entry.cost <= candidate.cost => {}
            _ => best[target_score] = Some(candidate),
        }
    }
}

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
        format_duration(Duration::from_secs_f64(remaining as f64 / speed.max(0.001)))
    };

    let ratio_percent = ratio * 100.0;
    let bar = format!("{:<width$}", "#".repeat(filled.min(PROGRESS_BAR_WIDTH)), width = PROGRESS_BAR_WIDTH);
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

fn start_progress_reporter(progress: Arc<ProgressTracker>) -> thread::JoinHandle<()> {
    print!("{}", render_progress_line(&progress));
    let _ = io::stdout().flush();

    thread::spawn(move || {
        while !progress.finished.load(Ordering::Relaxed) {
            thread::sleep(Duration::from_millis(100));
            print!("{}", render_progress_line(&progress));
            let _ = io::stdout().flush();
        }

        print!("{}", render_progress_line(&progress));
        println!();
    })
}

fn compute_best(progress: &ProgressTracker) -> BestTable {
    let def_pmf_table = build_def_pmf_table();
    let form_pmf_table = build_form_pmf_table();
    let def_cost_prefix = prefix_sums_u32(&DEFS_COSTS);
    let red_proof_form_cost_prefix = prefix_sums_f64(&RED_PROOFS_FORMS_COSTS);
    let black_proof_form_cost_prefix = prefix_sums_f64(&BLACK_PROOFS_FORMS_COSTS);
    let red_proof_body_cost_prefix = prefix_sums_u32(&RED_PROOFS_BODY_COSTS);
    let black_proof_body_cost_prefix = prefix_sums_u32(&BLACK_PROOFS_BODY_COSTS);
    let proof_configs = build_proof_configs(
        &red_proof_form_cost_prefix,
        &black_proof_form_cost_prefix,
        &red_proof_body_cost_prefix,
        &black_proof_body_cost_prefix,
    );

    proof_configs
        .par_iter()
        .map(|proof_config| {
            let mut best: BestTable = vec![None; TARGET_SCORE_COUNT];

            for k_def in 0..=DEFS_COSTS.len() {
                update_best_for_config(
                    &mut best,
                    def_cost_prefix[k_def],
                    &def_pmf_table[k_def],
                    &form_pmf_table,
                    k_def as u8,
                    proof_config,
                );
            }

            progress.completed.fetch_add(1, Ordering::Relaxed);

            best
        })
        .reduce(
            || vec![None; TARGET_SCORE_COUNT],
            |mut acc, best| {
                for score in 0..=MAX_TOTAL_SCORE {
                    if let Some(candidate) = best[score] {
                        if !acc[score].is_some_and(|entry| entry.cost <= candidate.cost) {
                            acc[score] = Some(candidate);
                        }
                    }
                }
                acc
            },
        )
}

fn cache_file_path() -> PathBuf {
    env::temp_dir()
        .join("math-strat")
        .join(format!("best-{CACHE_VERSION}.json"))
}

fn load_best_from_cache(path: &PathBuf) -> Option<BestTable> {
    let content = fs::read_to_string(path).ok()?;
    let best: BestTable = serde_json::from_str(&content).ok()?;
    if best.len() == MAX_TOTAL_SCORE + 1 {
        Some(best)
    } else {
        None
    }
}

fn save_best_to_cache(path: &PathBuf, best: &BestTable) {
    if let Some(parent) = path.parent() {
        let _ = fs::create_dir_all(parent);
    }

    if let Ok(content) = serde_json::to_string(best) {
        let _ = fs::write(path, content);
    }
}

fn load_or_compute_best() -> BestTable {
    let cache_path = cache_file_path();
    if let Some(best) = load_best_from_cache(&cache_path) {
        return best;
    }

    let progress = Arc::new(ProgressTracker {
        total: total_compute_iterations(),
        completed: AtomicU64::new(0),
        finished: AtomicBool::new(false),
        started_at: Instant::now(),
    });
    let reporter = start_progress_reporter(progress.clone());
    let best = compute_best(progress.as_ref());
    progress.completed.store(progress.total, Ordering::Relaxed);
    progress.finished.store(true, Ordering::Relaxed);
    let _ = reporter.join();
    save_best_to_cache(&cache_path, &best);
    best
}

fn read_target_score() -> Option<f64> {
    loop {
        print!("{} {}: ", "Введите желаемый балл", "(например 3.5)".cyan().bold());
        io::stdout().flush().expect("failed to flush stdout");

        let mut input = String::new();
        io::stdin()
            .read_line(&mut input)
            .expect("failed to read target score");

        let trimmed = input.trim();
        if trimmed.is_empty() {
            println!(
                "{}",
                status_line("warning:", Color::Yellow, "Пустой ввод. Попробуйте еще раз.")
            );
            continue;
        }

        let normalized = trimmed.replace(',', ".");
        match normalized.parse::<f64>() {
            Ok(value) if value >= 0.0 => return Some(value),
            _ => println!(
                "{} {}",
                status_line("error:", Color::Red, "Не удалось распознать число."),
                "Пример: 4 или 3.5".cyan().bold()
            ),
        }
    }
}

fn resolve_requested_score(target: f64) -> Option<usize> {
    if target > MAX_TOTAL_SCORE as f64 {
        return None;
    }

    Some(target.ceil() as usize)
}

fn pause_before_exit() {
    #[cfg(windows)]
    {
        print!("\nНажмите Enter, чтобы закрыть окно...");
        let _ = io::stdout().flush();
        let mut buffer = String::new();
        let _ = io::stdin().read_line(&mut buffer);
    }
}

fn main() {
    println!("{}", status_line("Запуск", Color::Magenta, "Стратегия на красный матан"));
    println!("{}", status_line("Кэш", Color::Cyan, "Происходит прогрев кэша (гоев)"));
    println!("{}", status_line("Кэш", Color::Cyan, "Немного подождите... (это один раз происходит)"));

    let started_at = Instant::now();
    let best = load_or_compute_best();
    println!(
        "{}",
        status_line(
            "Готово",
            Color::Green,
            format!("Подготовка завершена за {}.", format_duration(started_at.elapsed()).bold())
        )
    );

    println!(
        "{}",
        status_line(
            "Диапазон",
            Color::Blue,
            format!("от {} до {} баллов.", "0".bold(), MAX_TOTAL_SCORE.to_string().bold())
        )
    );
    println!(
        "{}",
        status_line(
            "Правило",
            Color::Blue,
            "Для дробного запроса используется ближайший больший целый балл.".dimmed()
        )
    );

    let target = read_target_score().expect("target score input unexpectedly missing");
    let requested_score = resolve_requested_score(target);

    match requested_score.and_then(|score| {
        best.get(score)
            .copied()
            .flatten()
            .map(|entry| (score, entry))
    }) {
        Some((score, entry)) => {
            println!(
                "\n{}",
                status_line(
                    "План",
                    Color::Green,
                    format!("на {} баллов (p90 сдаст)", score.to_string().bold())
                )
            );
            if (target - score as f64).abs() > f64::EPSILON {
                println!(
                    "{}",
                    status_line(
                        "warning:",
                        Color::Yellow,
                        format!(
                            "Запрошено {}, округлено вверх до {}.",
                            format_score(target).bold(),
                            score.to_string().bold()
                        )
                    )
                );
            }
            println!("{}", field_line("Опры", Color::Cyan, format_count(entry.k_def, TOTAL_DEF_CARDS)));
            println!(
                "{}",
                field_line(
                    "формул.",
                    Color::Red,
                    format_count(entry.k_red_pf, TOTAL_RED_PROOF_CARDS)
                )
            );
            println!(
                "{}",
                field_line(
                    "формул.",
                    Color::BrightBlack,
                    format_count(entry.k_black_pf, TOTAL_BLACK_PROOF_CARDS)
                )
            );
            println!(
                "{}",
                field_line(
                    "доки",
                    Color::Red,
                    format_count(entry.k_red_pp, TOTAL_RED_PROOF_CARDS)
                )
            );
            println!(
                "{}",
                field_line(
                    "доки",
                    Color::BrightBlack,
                    format_count(entry.k_black_pp, TOTAL_BLACK_PROOF_CARDS)
                )
            );
            println!("{}", field_line("Стоимость (условно)", Color::Green, format_cost(entry.cost).bold()));
        }
        None => println!(
            "\n{}",
            status_line(
                "error:",
                Color::Red,
                format!("Максимум {}", MAX_TOTAL_SCORE.to_string().bold())
            )
        ),
    }

    pause_before_exit();
}

fn format_cost(cost: f64) -> String {
    if (cost.fract() - 0.0).abs() < f64::EPSILON {
        format!("{:.0}", cost)
    } else {
        format!("{:.1}", cost)
    }
}

fn format_score(score: f64) -> String {
    if (score.fract() - 0.0).abs() < f64::EPSILON {
        format!("{:.0}", score)
    } else {
        format!("{:.1}", score)
    }
}

fn format_duration(duration: Duration) -> String {
    let total_ms = duration.as_millis();

    if total_ms < 1_000 {
        return format!("{} мс", total_ms);
    }

    let total_secs = duration.as_secs();
    if total_secs < 60 {
        return format!("{:.1} сек", duration.as_secs_f64());
    }

    let hours = total_secs / 3_600;
    let minutes = (total_secs % 3_600) / 60;
    let seconds = total_secs % 60;

    if hours > 0 {
        format!("{} ч {:02} мин {:02} сек", hours, minutes, seconds)
    } else {
        format!("{} мин {:02} сек", minutes, seconds)
    }
}
