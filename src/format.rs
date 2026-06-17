use std::time::Duration;

pub fn format_cost(cost: f64) -> String {
    if (cost.fract() - 0.0).abs() < f64::EPSILON {
        format!("{:.0}", cost)
    } else {
        format!("{:.1}", cost)
    }
}

pub fn format_score(score: f64) -> String {
    if (score.fract() - 0.0).abs() < f64::EPSILON {
        format!("{:.0}", score)
    } else {
        format!("{:.1}", score)
    }
}

pub fn format_duration(duration: Duration) -> String {
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
