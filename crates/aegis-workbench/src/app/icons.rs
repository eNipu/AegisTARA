//! Lucide icon names from `aegis-client` to the icons gpui-component
//! ships. Slice 182e adds the command-table icons it needs.

use gpui_component::IconName;

/// The icon for a Lucide name; a plain file for one we do not ship.
#[must_use]
pub fn icon(key: &str) -> IconName {
    match key {
        "book-open" => IconName::BookOpen,
        "square" => IconName::Square,
        "chart-pie" => IconName::ChartPie,
        "cpu" => IconName::Cpu,
        "triangle-alert" => IconName::TriangleAlert,
        "folder" => IconName::Folder,
        "folder-open" => IconName::FolderOpen,
        _ => IconName::File,
    }
}
