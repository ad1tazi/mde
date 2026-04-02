pub struct Clipboard {
    system: Option<arboard::Clipboard>,
    internal: String,
}

impl Clipboard {
    pub fn new() -> Self {
        let system = arboard::Clipboard::new().ok();
        Self {
            system,
            internal: String::new(),
        }
    }

    pub fn get_text(&mut self) -> String {
        if let Some(ref mut cb) = self.system {
            if let Ok(text) = cb.get_text() {
                return text;
            }
        }
        self.internal.clone()
    }

    pub fn set_text(&mut self, text: &str) {
        self.internal = text.to_string();
        if let Some(ref mut cb) = self.system {
            let _ = cb.set_text(text);
        }
    }
}
