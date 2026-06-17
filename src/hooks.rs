use std::io;

pub fn pause_before_exit() -> io::Result<()> {
    #[cfg(windows)]
    {
        use std::io::Write;

        let mut stdout = io::stdout();
        write!(stdout, "\nНажмите Enter, чтобы закрыть окно...")?;
        stdout.flush()?;
        let mut buffer = String::new();
        io::stdin().read_line(&mut buffer)?;
    }

    Ok(())
}
