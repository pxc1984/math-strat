pub fn pause_before_exit() {
    #[cfg(windows)]
    {
        print!("\nНажмите Enter, чтобы закрыть окно...");
        let _ = io::stdout().flush();
        let mut buffer = String::new();
        let _ = io::stdin().read_line(&mut buffer);
    }
}
