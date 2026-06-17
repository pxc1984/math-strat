use colored::{Color, Colorize};
use consts::{
    BLACK_PROOF_BODY_SCORE, BLACK_PROOF_FORMULATION_SCORE, BLACK_PROOFS_BODY_COSTS,
    BLACK_PROOFS_FORMS_COSTS, DEFS_COSTS, MAX_DRAWN_PROOF_FORMS, MAX_PROOF_FORM_CARDS,
    MAX_TOTAL_SCORE, RED_PROOF_BODY_SCORE, RED_PROOF_FORMULATION_SCORE, RED_PROOFS_BODY_COSTS,
    RED_PROOFS_FORMS_COSTS, TARGET_SCORE_COUNT, TOTAL_BLACK_PROOF_CARDS, TOTAL_DEF_CARDS,
    TOTAL_DEF_QUESTIONS, TOTAL_FORM_CARDS_AFTER_PROOFS, TOTAL_FORM_QUESTIONS,
    TOTAL_RED_PROOF_CARDS, TOTAL_RED_PROOF_QUESTIONS,
};
use progress::ProgressTracker;
use rayon::prelude::*;
use serde::{Deserialize, Serialize};
use std::process::ExitCode;
use std::sync::atomic::Ordering;
use std::time::Instant;

mod cache;
mod consts;
mod error;
mod format;
mod helpers;
mod hooks;
mod io;
mod progress;

use error::AppError;

type Outcomes = Vec<(u8, f64)>;
type Distribution = [f64; MAX_TOTAL_SCORE + 1];
type DefPmfTable = Vec<Outcomes>;
type FormPmfTable = Vec<Vec<Outcomes>>;

#[derive(Clone, Copy, Serialize, Deserialize)]
pub struct BestEntry {
    cost: f64,
    k_def: u8,
    k_red_pf: u8,
    k_black_pf: u8,
    k_red_pp: u8,
    k_black_pp: u8,
}

pub type BestTable = Vec<Option<BestEntry>>;

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

fn build_def_pmf_table() -> DefPmfTable {
    (0..=DEFS_COSTS.len() as u8).map(build_def_pmf).collect()
}

fn build_def_pmf(k_def: u8) -> Outcomes {
    let k_def = k_def as u32;
    let total = helpers::comb(TOTAL_DEF_CARDS, TOTAL_DEF_QUESTIONS);
    let mut outcomes = Vec::with_capacity(TOTAL_DEF_QUESTIONS as usize + 1);

    for x in 0..=TOTAL_DEF_QUESTIONS {
        if k_def < x || TOTAL_DEF_CARDS - k_def < TOTAL_DEF_QUESTIONS - x {
            continue;
        }

        let prob = helpers::comb(k_def, x)
            * helpers::comb(TOTAL_DEF_CARDS - k_def, TOTAL_DEF_QUESTIONS - x)
            / total;
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

    let total_ways = helpers::comb(TOTAL_FORM_CARDS_AFTER_PROOFS, TOTAL_FORM_QUESTIONS);
    let mut outcomes = Vec::with_capacity(TOTAL_FORM_QUESTIONS as usize + 1);

    for x in 0..=TOTAL_FORM_QUESTIONS {
        if known_remaining < x
            || TOTAL_FORM_CARDS_AFTER_PROOFS - known_remaining < TOTAL_FORM_QUESTIONS - x
        {
            continue;
        }

        let prob = helpers::comb(known_remaining, x)
            * helpers::comb(
                TOTAL_FORM_CARDS_AFTER_PROOFS - known_remaining,
                TOTAL_FORM_QUESTIONS - x,
            )
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
    let total_red_proof_ways = helpers::comb(TOTAL_RED_PROOF_CARDS, TOTAL_RED_PROOF_QUESTIONS);
    let black_total = TOTAL_BLACK_PROOF_CARDS as f64;
    let mut outcomes = Vec::with_capacity(16);

    for red_full in 0..=TOTAL_RED_PROOF_QUESTIONS {
        if red_full > k_red_pp as u32 {
            break;
        }

        let max_red_form_only =
            (TOTAL_RED_PROOF_QUESTIONS - red_full).min((k_red_pf - k_red_pp) as u32);
        for red_form_only in 0..=max_red_form_only {
            let red_unknown = TOTAL_RED_PROOF_QUESTIONS - red_full - red_form_only;
            if red_unknown > TOTAL_RED_PROOF_CARDS - k_red_pf as u32 {
                continue;
            }

            let red_prob = helpers::comb(k_red_pp as u32, red_full)
                * helpers::comb((k_red_pf - k_red_pp) as u32, red_form_only)
                * helpers::comb(TOTAL_RED_PROOF_CARDS - k_red_pf as u32, red_unknown)
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
        let form_outcomes = form_pmf(
            form_pmf_table,
            proof_config.total_pf,
            outcome.drawn_pf_known,
        );
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
            let proof_form_cost =
                red_proof_form_cost_prefix[k_red_pf] + black_proof_form_cost_prefix[k_black_pf];
            for k_red_pp in 0..=k_red_pf {
                for k_black_pp in 0..=k_black_pf {
                    let proof_body_cost = red_proof_body_cost_prefix[k_red_pp]
                        + black_proof_body_cost_prefix[k_black_pp];
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

pub fn compute_best(progress: &ProgressTracker) -> BestTable {
    let def_pmf_table = build_def_pmf_table();
    let form_pmf_table = build_form_pmf_table();
    let def_cost_prefix = helpers::prefix_sums(&DEFS_COSTS);
    let red_proof_form_cost_prefix = helpers::prefix_sums(&RED_PROOFS_FORMS_COSTS);
    let black_proof_form_cost_prefix = helpers::prefix_sums(&BLACK_PROOFS_FORMS_COSTS);
    let red_proof_body_cost_prefix = helpers::prefix_sums(&RED_PROOFS_BODY_COSTS);
    let black_proof_body_cost_prefix = helpers::prefix_sums(&BLACK_PROOFS_BODY_COSTS);
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

fn resolve_requested_score(target: f64) -> Option<usize> {
    if target > MAX_TOTAL_SCORE as f64 {
        return None;
    }

    Some(target.ceil() as usize)
}

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            if let Err(report_error) =
                io::write_error_line(io::status_line("error:", Color::Red, format!("{error}")))
            {
                drop(report_error);
            }

            if let Err(pause_error) = hooks::pause_before_exit() {
                drop(pause_error);
            }

            ExitCode::FAILURE
        }
    }
}

fn run() -> Result<(), AppError> {
    io::write_line(io::status_line(
        "Запуск",
        Color::Magenta,
        "Стратегия на красный матан",
    ))?;
    io::write_line(io::status_line(
        "Кэш",
        Color::Cyan,
        "Происходит прогрев кэша (гоев)",
    ))?;
    io::write_line(io::status_line(
        "Кэш",
        Color::Cyan,
        "Немного подождите... (это один раз происходит)",
    ))?;

    let started_at = Instant::now();
    let best = cache::load_or_compute_best()?;
    io::write_line(io::status_line(
        "Готово",
        Color::Green,
        format!(
            "Подготовка завершена за {}.",
            format::format_duration(started_at.elapsed()).bold()
        ),
    ))?;

    io::write_line(io::status_line(
        "Диапазон",
        Color::Blue,
        format!(
            "от {} до {} баллов.",
            "0".bold(),
            MAX_TOTAL_SCORE.to_string().bold()
        ),
    ))?;
    io::write_line(io::status_line(
        "Правило",
        Color::Blue,
        "Для дробного запроса используется ближайший больший целый балл.".dimmed(),
    ))?;

    let target = io::read_target_score()?;
    let requested_score = resolve_requested_score(target);

    match requested_score.and_then(|score| {
        best.get(score)
            .copied()
            .flatten()
            .map(|entry| (score, entry))
    }) {
        Some((score, entry)) => {
            io::write_line(format!(
                "\n{}",
                io::status_line(
                    "План",
                    Color::Green,
                    format!("на {} баллов (p90 сдаст)", score.to_string().bold())
                )
            ))?;
            if (target - score as f64).abs() > f64::EPSILON {
                io::write_line(io::status_line(
                    "warning:",
                    Color::Yellow,
                    format!(
                        "Запрошено {}, округлено вверх до {}.",
                        format::format_score(target).bold(),
                        score.to_string().bold()
                    ),
                ))?;
            }
            io::write_line(io::field_line(
                "Опры",
                Color::Cyan,
                io::format_count(entry.k_def, TOTAL_DEF_CARDS),
            ))?;
            io::write_line(io::field_line(
                "формул.",
                Color::Red,
                io::format_count(entry.k_red_pf, TOTAL_RED_PROOF_CARDS),
            ))?;
            io::write_line(io::field_line(
                "формул.",
                Color::BrightBlack,
                io::format_count(entry.k_black_pf, TOTAL_BLACK_PROOF_CARDS),
            ))?;
            io::write_line(io::field_line(
                "доки",
                Color::Red,
                io::format_count(entry.k_red_pp, TOTAL_RED_PROOF_CARDS),
            ))?;
            io::write_line(io::field_line(
                "доки",
                Color::BrightBlack,
                io::format_count(entry.k_black_pp, TOTAL_BLACK_PROOF_CARDS),
            ))?;
            io::write_line(io::field_line(
                "Стоимость (условно)",
                Color::Green,
                format::format_cost(entry.cost).bold(),
            ))?;
        }
        None => io::write_line(format!(
            "\n{}",
            io::status_line(
                "error:",
                Color::Red,
                format!("Максимум {}", MAX_TOTAL_SCORE.to_string().bold())
            )
        ))?,
    }

    hooks::pause_before_exit()?;

    Ok(())
}
