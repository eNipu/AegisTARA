//! Lucide icon names from `aegis-client` (explorer rows and the command
//! table) to the icons gpui-component ships.

use gpui_component::IconName;

/// The icon for a Lucide name; `None` for one we do not ship.
#[must_use]
pub fn icon(key: &str) -> Option<IconName> {
    Some(match key {
        "book-open" => IconName::BookOpen,
        "square" => IconName::Square,
        "chart-pie" => IconName::ChartPie,
        "cpu" => IconName::Cpu,
        "triangle-alert" => IconName::TriangleAlert,
        "folder" => IconName::Folder,
        "folder-open" => IconName::FolderOpen,
        "file" => IconName::File,
        "hard-drive" => IconName::HardDrive,
        "search" => IconName::Search,
        "undo" => IconName::Undo,
        "redo" => IconName::Redo,
        "panel-left" => IconName::PanelLeft,
        "panel-right" => IconName::PanelRight,
        "panel-bottom" => IconName::PanelBottom,
        "case-sensitive" => IconName::CaseSensitive,
        "arrow-down" => IconName::ArrowDown,
        "arrow-up" => IconName::ArrowUp,
        "arrow-left" => IconName::ArrowLeft,
        "arrow-right" => IconName::ArrowRight,
        "chevron-left" => IconName::ChevronLeft,
        "chevron-right" => IconName::ChevronRight,
        "external-link" => IconName::ExternalLink,
        "copy" => IconName::Copy,
        "inbox" => IconName::Inbox,
        "close" => IconName::Close,
        "moon" => IconName::Moon,
        "layout-dashboard" => IconName::LayoutDashboard,
        "info" => IconName::Info,
        "window-close" => IconName::WindowClose,
        _ => return None,
    })
}
