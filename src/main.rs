use cached::proc_macro::cached;
use serde::{Deserialize, Serialize};
use std::env;
use std::fs;
use std::io::{self, Write};
use std::path::PathBuf;
use std::time::{Duration, Instant};

const DEFS_COSTS: [u32; 24] = [1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 2, 2, 2, 2, 2, 4, 4, 4, 4, 4];
const FORMS_COSTS: [f64; 49] = [
    1.0, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0,
    1.0, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0, 2.0, 2.0, 2.0, 2.0, 2.0, 2.0, 2.0, 2.0, 2.0,
    2.0, 2.0, 2.0, 2.0, 2.0, 3.5, 3.5, 3.5, 3.5, 3.5, 3.5, 3.5, 3.5, 3.5, 3.5,
];
const PROOFS_FORMS_COSTS: [f64; 33] = [
    1.0, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0, 2.0, 2.0,
    2.0, 2.0, 2.0, 2.0, 2.0, 2.0, 2.0, 2.0, 3.0, 3.0, 3.0, 3.0, 3.0, 3.0, 3.0, 3.0,
];
const PROOFS_BODY_COSTS: [u32; 33] = [2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 7, 7, 7, 7, 7, 7, 7, 7, 7, 7];

const MAX_TOTAL_SCORE: usize = 18;
const TOTAL_DEF_CARDS: u32 = 24;
const TOTAL_FORM_CARDS_AFTER_PROOFS: u32 = 45;
const TOTAL_PROOF_CARDS: u32 = 33;

type Outcomes = Vec<(u8, f64)>;
type Distribution = [f64; MAX_TOTAL_SCORE + 1];

const CACHE_VERSION: &str = "v1";

#[derive(Clone, Copy, Serialize, Deserialize)]
struct BestEntry {
    cost: f64,
    k_def: u8,
    k_extra: u8,
    k_pf: u8,
    k_pp: u8,
}

type BestTable = Vec<Option<BestEntry>>;

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

#[cached]
fn def_pmf(k_def: u8) -> Outcomes {
    let k_def = k_def as u32;
    let total = comb(TOTAL_DEF_CARDS, 3);
    let mut outcomes = Vec::with_capacity(4);

    for x in 0..=3 {
        if k_def < x || TOTAL_DEF_CARDS - k_def < 3 - x {
            continue;
        }

        let prob = comb(k_def, x) * comb(TOTAL_DEF_CARDS - k_def, 3 - x) / total;
        if prob > 0.0 {
            outcomes.push((x as u8, prob));
        }
    }

    outcomes
}

#[cached]
fn form_pmf(k_pf: u8, k_extra: u8, drawn_pf_known: u8) -> Outcomes {
    let k_pf = k_pf as u32;
    let k_extra = k_extra as u32;
    let drawn_pf_known = drawn_pf_known as u32;
    let known_remaining = k_pf.saturating_sub(drawn_pf_known) + k_extra;

    if known_remaining > TOTAL_FORM_CARDS_AFTER_PROOFS {
        return Vec::new();
    }

    if known_remaining == 0 {
        return vec![(0, 1.0)];
    }

    let total_ways = comb(TOTAL_FORM_CARDS_AFTER_PROOFS, 2);
    let mut outcomes = Vec::with_capacity(3);

    for x in 0..=2 {
        if known_remaining < x || TOTAL_FORM_CARDS_AFTER_PROOFS - known_remaining < 2 - x {
            continue;
        }

        let prob = comb(known_remaining, x)
            * comb(TOTAL_FORM_CARDS_AFTER_PROOFS - known_remaining, 2 - x)
            / total_ways;
        if prob > 0.0 {
            outcomes.push((x as u8, prob));
        }
    }

    outcomes
}

fn ticket_pmf(k_def: u8, k_pf: u8, k_pp: u8, k_extra: u8) -> Distribution {
    let def_outcomes = def_pmf(k_def);
    let total_proof_ways = comb(TOTAL_PROOF_CARDS, 4);
    let mut distribution = [0.0; MAX_TOTAL_SCORE + 1];

    for a in 0..=4u32 {
        if a > k_pp as u32 {
            break;
        }

        let max_b = (4 - a).min((k_pf - k_pp) as u32);
        for b in 0..=max_b {
            let c = 4 - a - b;
            if c > TOTAL_PROOF_CARDS - k_pf as u32 {
                continue;
            }

            let proof_prob = comb(k_pp as u32, a)
                * comb((k_pf - k_pp) as u32, b)
                * comb(TOTAL_PROOF_CARDS - k_pf as u32, c)
                / total_proof_ways;
            if proof_prob == 0.0 {
                continue;
            }

            let drawn_pf_known = (a + b) as u8;
            let form_outcomes = form_pmf(k_pf, k_extra, drawn_pf_known);

            if a > 0 {
                let with_body_score = (3 * a + b + 1) as usize;
                let without_body_score = (3 * a + b) as usize;
                let with_body_prob = proof_prob * (a as f64 / 4.0);
                let without_body_prob = proof_prob - with_body_prob;

                accumulate_scores(
                    &mut distribution,
                    with_body_score,
                    with_body_prob,
                    &form_outcomes,
                    &def_outcomes,
                );
                if without_body_prob > 0.0 {
                    accumulate_scores(
                        &mut distribution,
                        without_body_score,
                        without_body_prob,
                        &form_outcomes,
                        &def_outcomes,
                    );
                }
            } else {
                accumulate_scores(
                    &mut distribution,
                    b as usize,
                    proof_prob,
                    &form_outcomes,
                    &def_outcomes,
                );
            }
        }
    }

    distribution
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

fn compute_best() -> BestTable {
    let def_cost_prefix = prefix_sums_u32(&DEFS_COSTS);
    let form_cost_prefix = prefix_sums_f64(&FORMS_COSTS);
    let proof_form_cost_prefix = prefix_sums_f64(&PROOFS_FORMS_COSTS);
    let proof_body_cost_prefix = prefix_sums_u32(&PROOFS_BODY_COSTS);

    let mut best: BestTable = vec![None; MAX_TOTAL_SCORE + 1];

    for k_def in 0..=DEFS_COSTS.len() {
        let def_cost = def_cost_prefix[k_def];
        for k_pf in 0..=PROOFS_FORMS_COSTS.len() {
            let proof_form_cost = proof_form_cost_prefix[k_pf];
            for k_pp in 0..=k_pf {
                let proof_body_cost = proof_body_cost_prefix[k_pp];
                for k_extra in 0..=(FORMS_COSTS.len() - k_pf) {
                    let total_cost =
                        def_cost + proof_form_cost + proof_body_cost + form_cost_prefix[k_extra];

                    let distribution = ticket_pmf(k_def as u8, k_pf as u8, k_pp as u8, k_extra as u8);
                    let score = check_score(&distribution);

                    match best[score] {
                        Some(entry) if entry.cost <= total_cost => {}
                        _ => {
                            best[score] = Some(BestEntry {
                                cost: total_cost,
                                k_def: k_def as u8,
                                k_extra: k_extra as u8,
                                k_pf: k_pf as u8,
                                k_pp: k_pp as u8,
                            })
                        }
                    }
                }
            }
        }
    }

    best
}

fn cache_file_path() -> PathBuf {
    env::temp_dir().join("math-strat").join(format!("best-{CACHE_VERSION}.json"))
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

    let best = compute_best();
    save_best_to_cache(&cache_path, &best);
    best
}

fn read_target_score() -> Option<f64> {
    loop {
        print!("Введите желаемый балл (можно дробный: 17.5 или 17,5): ");
        io::stdout().flush().expect("failed to flush stdout");

        let mut input = String::new();
        io::stdin()
            .read_line(&mut input)
            .expect("failed to read target score");

        let trimmed = input.trim();
        if trimmed.is_empty() {
            println!("Пустой ввод. Попробуйте еще раз.");
            continue;
        }

        let normalized = trimmed.replace(',', ".");
        match normalized.parse::<f64>() {
            Ok(value) if value >= 0.0 => return Some(value),
            _ => println!("Не удалось распознать число. Пример: 18 или 17.5"),
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
    println!("Math Strat");
    println!("Ищу самый дешевый план на заданный результат с вероятностью 90%.");
    println!("Немного подождите...");

    let started_at = Instant::now();
    let best = load_or_compute_best();
    println!("Подготовка завершена за {}.", format_duration(started_at.elapsed()));

    println!("Доступный диапазон: от 0 до {MAX_TOTAL_SCORE} баллов.");
    println!("Для дробного запроса используется ближайший больший целый балл.");

    let target = read_target_score().expect("target score input unexpectedly missing");
    let requested_score = resolve_requested_score(target);

    match requested_score.and_then(|score| best.get(score).copied().flatten().map(|entry| (score, entry))) {
        Some((score, entry)) => {
            println!("\nПлан на {} баллов (90%):", score);
            if (target - score as f64).abs() > f64::EPSILON {
                println!("Запрошено: {}, округлено вверх до {}.", format_score(target), score);
            }
            println!("Опры: {} шт", entry.k_def);
            println!("Формулировки к докам: {} шт", entry.k_pf);
            println!("Чистые формулировки: {} шт", entry.k_extra);
            println!("Доки: {} шт", entry.k_pp);
            println!("Стоимость: {}", format_cost(entry.cost));
        }
        None => println!("\nМаксимум {MAX_TOTAL_SCORE}"),
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
