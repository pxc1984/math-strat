use colored::{Color, ColoredString, Colorize};
use std::fmt::Display;
use std::io::{self, Write};

pub fn read_target_score() -> io::Result<f64> {
    loop {
        let mut stdout = io::stdout();
        write!(
            stdout,
            "{} {}: ",
            "Введите желаемый балл",
            "(например 3.5)".cyan().bold()
        )?;
        stdout.flush()?;

        let mut input = String::new();
        let bytes_read = io::stdin().read_line(&mut input)?;
        if bytes_read == 0 {
            return Err(io::Error::new(
                io::ErrorKind::UnexpectedEof,
                "ввод прерван до чтения целевого балла",
            ));
        }

        let trimmed = input.trim();
        if trimmed.is_empty() {
            write_line(status_line(
                "warning:",
                Color::Yellow,
                "Пустой ввод. Попробуйте еще раз.",
            ))?;
            continue;
        }

        let normalized = trimmed.replace(',', ".");
        match normalized.parse::<f64>() {
            Ok(value) if value >= 0.0 => return Ok(value),
            _ => write_line(format!(
                "{} {}",
                status_line("error:", Color::Red, "Не удалось распознать число."),
                "Пример: 4 или 3.5".cyan().bold()
            ))?,
        }
    }
}

pub fn write_line(message: impl Display) -> io::Result<()> {
    let mut stdout = io::stdout();
    writeln!(stdout, "{message}")
}

pub fn write_error_line(message: impl Display) -> io::Result<()> {
    let mut stderr = io::stderr();
    writeln!(stderr, "{message}")
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
