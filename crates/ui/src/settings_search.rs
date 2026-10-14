#![deny(warnings)]
#![forbid(unsafe_code)]

use crate::command::{CommandId, CommandRegistry};
use crate::settings::SettingsPane;

pub const MAX_RESULTS: usize = 8;
const BREADCRUMB: &str = "\u{203A}";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SettingsRow {
    Theme,
    CustomizeColors,
    InterfaceSize,
    DimPanes,
    Font,
    TextSize,
    EditorPosition,
    EditorSize,
}

impl SettingsRow {
    pub const ALL: [Self; 8] = [
        Self::Theme, Self::CustomizeColors, Self::InterfaceSize, Self::DimPanes, Self::Font, Self::TextSize,
        Self::EditorPosition, Self::EditorSize,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Self::Theme => "Theme",
            Self::CustomizeColors => "Customize Colors",
            Self::InterfaceSize => "Interface size",
            Self::DimPanes => "Dim inactive panes",
            Self::Font => "Font",
            Self::TextSize => "Text size",
            Self::EditorPosition => "Editor position",
            Self::EditorSize => "Editor size",
        }
    }

    pub fn pane(self) -> SettingsPane {
        match self {
            Self::Theme | Self::CustomizeColors | Self::InterfaceSize | Self::DimPanes => SettingsPane::Appearance,
            Self::Font | Self::TextSize => SettingsPane::Terminal,
            Self::EditorPosition | Self::EditorSize => SettingsPane::Editor,
        }
    }

    fn keywords(self) -> &'static str {
        match self {
            Self::Theme => "color colour dark light mode preset",
            Self::CustomizeColors => "custom palette ansi accent background",
            Self::InterfaceSize => "scale zoom tabs menus ui",
            Self::DimPanes => "focus fade unfocused",
            Self::Font => "typeface family monospace",
            Self::TextSize => "font size points zoom",
            Self::EditorPosition => "bundle left right bottom layout",
            Self::EditorSize => "bundle width height split",
        }
    }

    fn matches(self, needle: &str) -> bool {
        format!("{} {}", self.label(), self.keywords()).to_lowercase().contains(needle)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SearchTarget {
    Setting(SettingsRow),
    Shortcut(CommandId),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SearchResult {
    pub target: SearchTarget,
    pub label: &'static str,
    pub caption: String,
}

pub fn search_settings(query: &str, registry: &CommandRegistry) -> Vec<SearchResult> {
    let needle = query.trim().to_lowercase();
    if needle.is_empty() {
        return Vec::new();
    }
    let settings = SettingsRow::ALL.into_iter().filter(|row| row.matches(&needle)).map(|row| SearchResult {
        target: SearchTarget::Setting(row),
        label: row.label(),
        caption: format!("{} {BREADCRUMB} {}", row.pane().label(), row.label()),
    });
    let rebindable = registry.list().into_iter().filter(|command| command.id.is_rebindable());
    let shortcuts = rebindable.filter(|command| command.matches(&needle)).map(|command| SearchResult {
        target: SearchTarget::Shortcut(command.id),
        label: command.title,
        caption: format!("{} {BREADCRUMB} {}", SettingsPane::Keyboard.label(), command.category),
    });
    settings.chain(shortcuts).take(MAX_RESULTS).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn labels(query: &str) -> Vec<&'static str> {
        search_settings(query, &CommandRegistry::with_defaults()).iter().map(|result| result.label).collect()
    }

    #[test]
    fn blank_queries_find_nothing() {
        assert!(labels("").is_empty());
        assert!(labels("   ").is_empty());
    }

    #[test]
    fn settings_match_labels_and_keywords_ignoring_case() {
        assert_eq!(labels("FONT"), ["Font", "Text size"]);
        assert_eq!(labels("colour"), ["Theme"]);
        assert_eq!(labels("zoom"), ["Interface size", "Text size"]);
    }

    #[test]
    fn settings_come_before_shortcut_matches() {
        assert_eq!(labels("split"), ["Editor size", "Split Right", "Split Down"]);
        let results = search_settings("split", &CommandRegistry::with_defaults());
        assert_eq!(results[0].target, SearchTarget::Setting(SettingsRow::EditorSize));
        assert_eq!(results[0].caption, "Editor \u{203A} Editor size");
        assert_eq!(results[1].target, SearchTarget::Shortcut(CommandId::SplitRight));
        assert_eq!(results[1].caption, "Keyboard \u{203A} Panes");
    }

    #[test]
    fn context_only_commands_are_not_offered() {
        assert_eq!(labels("palette"), ["Customize Colors", "Open Palette"]);
    }

    #[test]
    fn results_are_capped() {
        assert_eq!(labels("pane").len(), MAX_RESULTS);
        assert_eq!(labels("pane")[0], "Dim inactive panes");
    }
}
