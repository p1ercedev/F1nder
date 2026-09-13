#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Scope {
    Global,
    Search,
    Browse,
    Methodology,
    Fill,
    FileFilter,
    ModePicker,
    Recents,
    Profiles,
}

pub const KEYS: &[(Scope, &str, &str)] = &[
    (Scope::Global, "F1", "help"),
    (Scope::Global, "[ ]", "switch tab"),
    (Scope::Global, "Esc", "back / quit"),
    (Scope::Search, "⌘N", "navigation"),
    (Scope::Search, "⇥", "complete / mode"),
    (Scope::Search, "Enter", "copy"),
    (Scope::Search, "^Y", "copy & exit"),
    (Scope::Search, "⌘S", "favorite"),
    (Scope::Search, "⌘F", "file filter"),
    (Scope::Search, "^R", "recents"),
    (Scope::Global, "^P", "profile"),
    (Scope::Browse, "↑↓", "move"),
    (Scope::Browse, "⇥", "complete / mode"),
    (Scope::Browse, "h/l", "fold"),
    (Scope::Browse, "Enter", "copy"),
    (Scope::Browse, "^Y", "copy & exit"),
    (Scope::Methodology, "hjkl", "move"),
    (Scope::Methodology, "⌘F", "document"),
    (Scope::Methodology, "Tab/1-9", "section"),
    (Scope::Methodology, "gg/G", "ends"),
    (Scope::Methodology, "Space", "check"),
    (Scope::Methodology, "e/a/d", "edit"),
    (Scope::Methodology, "R/c", "reset/comments"),
    (Scope::Methodology, "/", "jump"),
    (Scope::Methodology, "o", "commands"),
    (Scope::Fill, "⏎", "next / copy"),
    (Scope::Fill, "⇥/→", "complete"),
    (Scope::Fill, "^X", "drop"),
    (Scope::Fill, "^A", "add arg"),
    (Scope::Fill, "^U", "clear to template"),
    (Scope::Fill, "^D", "base template"),
    (Scope::Fill, "^P/^N", "suggestion"),
    (Scope::Fill, "1–9", "pick suggestion"),
    (Scope::Fill, "^Space", "suggestions"),
    (Scope::Fill, "^T", "target"),
    (Scope::Fill, "^Y", "copy & exit"),
    (Scope::FileFilter, "0-9", "toggle file"),
    (Scope::FileFilter, "Esc", "close"),
    (Scope::ModePicker, "1-5", "pick mode"),
    (Scope::ModePicker, "Esc", "close"),
    (Scope::Recents, "j/k", "move"),
    (Scope::Recents, "Enter", "reopen"),
    (Scope::Recents, "Esc", "close"),
    (Scope::Profiles, "j/k", "move"),
    (Scope::Profiles, "Enter", "switch"),
    (Scope::Profiles, "n", "new"),
    (Scope::Profiles, "d", "delete"),
    (Scope::Profiles, "Esc", "close"),
];

impl Scope {
    /// Human-readable section name for the help overlay.
    pub fn label(self) -> &'static str {
        match self {
            Scope::Global => "Anywhere",
            Scope::Search => "Search",
            Scope::Browse => "Browse",
            Scope::Methodology => "Methodology",
            Scope::Fill => "Fill dialog",
            Scope::FileFilter => "File filter",
            Scope::ModePicker => "Search mode",
            Scope::Recents => "Recents",
            Scope::Profiles => "Profiles",
        }
    }
}

/// Every scope, in the order the help overlay shows them.
pub const ALL_SCOPES: &[Scope] = &[
    Scope::Global,
    Scope::Search,
    Scope::Browse,
    Scope::Methodology,
    Scope::Fill,
    Scope::FileFilter,
    Scope::ModePicker,
    Scope::Recents,
    Scope::Profiles,
];

pub fn hint(scope: Scope) -> String {
    KEYS.iter()
        .filter(|(s, _, _)| *s == scope)
        .map(|(_, k, d)| format!("{k} {d}"))
        .collect::<Vec<_>>()
        .join(" · ")
}
