use colored::{Color, ColoredString, Colorize};
use std::io::Write;

pub fn read_target_score() -> Option<f64> {
    loop {
        print!(
            "{} {}: ",
            "Введите желаемый балл",
            "(например 3.5)".cyan().bold()
        );
        std::io::stdout().flush().expect("failed to flush stdout");

        let mut input = String::new();
        std::io::stdin()
            .read_line(&mut input)
            .expect("failed to read target score");

        let trimmed = input.trim();
        if trimmed.is_empty() {
            println!(
                "{}",
                status_line(
                    "warning:",
                    Color::Yellow,
                    "Пустой ввод. Попробуйте еще раз."
                )
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

pub fn label(text: &str, color: Color) -> ColoredString {
    text.color(color).bold()
}

pub fn status_line(text: &str, color: Color, message: impl std::fmt::Display) -> String {
    format!("{:>12} {}", label(text, color), message)
}

pub fn field_line(text: &str, color: Color, value: impl std::fmt::Display) -> String {
    format!("{:>12} {}", label(text, color), value)
}

pub fn format_count(chosen: u8, total: u32) -> String {
    format!("{}/{} шт", chosen.to_string().bold(), total)
}
