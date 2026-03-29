fn main() {
    if let Err(error) = autoloop::run() {
        eprintln!("{}", autoloop::ui::render_error(&error));
        std::process::exit(1);
    }
}
