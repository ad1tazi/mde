mod app;
mod clipboard;
mod editor;
mod filetree;
mod finder;
mod input;
mod markdown;
mod render;
mod sidebar;

use app::App;

fn main() -> std::io::Result<()> {
    let file_path = std::env::args().nth(1);

    let mut terminal = ratatui::init();
    let result = App::new(file_path.as_deref())?.run(&mut terminal);
    ratatui::restore();

    result
}
