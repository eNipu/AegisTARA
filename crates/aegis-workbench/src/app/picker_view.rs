//! Reusable, keyboard-operable GPUI multi-link picker (UI-15) over
//! [`aegis_client::picker::MultiLinkPicker`].

use std::rc::Rc;

use aegis_client::commands::Edit;
use aegis_client::i18n::Catalog;
use aegis_client::picker::MultiLinkPicker;
use aegis_client::relations::RelationKey;
use aegis_client::view::Selection;
use aegis_model::Project;
use gpui::prelude::FluentBuilder as _;
use gpui::{
    App, AppContext as _, Context, Entity, FocusHandle, Focusable, InteractiveElement as _,
    IntoElement, KeyDownEvent, ParentElement as _, Render, SharedString,
    StatefulInteractiveElement as _, Styled as _, Subscription, WeakEntity, Window, div, px,
};
use gpui_component::button::{Button, ButtonVariants as _};
use gpui_component::input::{Input, InputEvent, InputState};
use gpui_component::{ActiveTheme as _, Icon, IconName, Sizable as _};

use super::workbench::Workbench;

/// Maximum number of candidate rows shown in the picker's bounded list.
const WINDOW_CAPACITY: usize = 10;

/// Callback invoked when the picker is dismissed (for example on Escape).
type CloseCallback = Box<dyn Fn(&mut Window, &mut App)>;

/// Keyboard-operable GPUI view wrapping [`MultiLinkPicker`] (UI-15).
///
/// Uses `capture_key_down` so navigation and commit keys (`up`, `down`,
/// `enter`, `escape`) work while the filter [`Input`] is focused without
/// interfering with typing or spaces in multi-word filter queries. All
/// workspace mutations and target jumps go through `Workbench::apply_edit_skipping`
/// and `Workbench::open_target_skipping` with this view's [`gpui::EntityId`]
/// so synchronous workbench broadcasts never double-lease this view.
pub struct MultiLinkPickerView {
    picker: MultiLinkPicker,
    filter: Entity<InputState>,
    catalog: Rc<Catalog>,
    workbench: WeakEntity<Workbench>,
    focus_handle: FocusHandle,
    on_close: Option<CloseCallback>,
    _sub: Subscription,
}

impl Focusable for MultiLinkPickerView {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl MultiLinkPickerView {
    /// Builds the view from an already-opened [`MultiLinkPicker`], letting
    /// callers drop any read borrow on [`Workbench`] before calling `cx.new`.
    pub fn from_picker(
        picker: MultiLinkPicker,
        catalog: Rc<Catalog>,
        workbench: WeakEntity<Workbench>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let placeholder = SharedString::from(catalog.tr("picker-placeholder"));
        let filter = cx.new(|cx| InputState::new(window, cx).placeholder(placeholder));
        let _sub = cx.subscribe(&filter, |this, input, ev: &InputEvent, cx| {
            if let InputEvent::Change = ev {
                let val = input.read(cx).value().to_string();
                if val != this.picker.filter().committed() {
                    this.picker.set_filter(val);
                    cx.notify();
                }
            }
        });
        filter.update(cx, |state, cx| state.focus(window, cx));
        Self {
            picker,
            filter,
            catalog,
            workbench,
            focus_handle: cx.focus_handle(),
            on_close: None,
            _sub,
        }
    }

    /// Opens a [`MultiLinkPicker`] for `from` on relation `key` and wraps it
    /// in a focused [`MultiLinkPickerView`].
    pub fn new(
        project: &Project,
        key: RelationKey,
        from: Selection,
        catalog: Rc<Catalog>,
        workbench: WeakEntity<Workbench>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        Self::from_picker(
            MultiLinkPicker::open(project, key, from),
            catalog,
            workbench,
            window,
            cx,
        )
    }

    /// Applies a picker edit through `Workbench::apply_edit_skipping` so this
    /// view is never re-leased during broadcast, then refreshes the live
    /// picker state immediately while keeping it open and filter intact.
    fn apply_picker_edit(&mut self, edit: Edit, cx: &mut Context<Self>) -> bool {
        let Some(wb) = self.workbench.upgrade() else {
            return false;
        };
        let my_id = cx.entity_id();
        let ok = wb.update(cx, |w, cx| w.apply_edit_skipping(&edit, Some(my_id), cx));
        self.picker.refresh(wb.read(cx).workspace().project());
        cx.notify();
        ok
    }

    /// Re-reads candidate options and link states from `project`, keeping the
    /// picker open, filter intact, and cursor clamped.
    pub fn refresh(&mut self, project: &Project, cx: &mut Context<Self>) {
        self.picker.refresh(project);
        cx.notify();
    }

    /// The underlying client-level picker state.
    #[must_use]
    pub fn picker(&self) -> &MultiLinkPicker {
        &self.picker
    }

    /// The filter input entity.
    #[must_use]
    pub fn filter_input(&self) -> &Entity<InputState> {
        &self.filter
    }

    /// Replaces the filter text in both the picker state and the filter input.
    pub fn set_filter(
        &mut self,
        text: impl Into<String>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let text = text.into();
        self.picker.set_filter(text.clone());
        self.filter
            .update(cx, |input, cx| input.set_value(text, window, cx));
        cx.notify();
    }

    /// Registers a callback invoked when the picker closes (such as on Escape).
    pub fn set_on_close(&mut self, f: impl Fn(&mut Window, &mut App) + 'static) {
        self.on_close = Some(Box::new(f));
    }

    /// Handles a logical picker key (`"down"`, `"up"`, `"enter"`, `"space"`,
    /// `"escape"`). Returns `true` when the key was handled.
    pub fn handle_key(&mut self, key: &str, window: &mut Window, cx: &mut Context<Self>) -> bool {
        match key {
            "down" => {
                self.picker.move_cursor(1);
                cx.notify();
                true
            }
            "up" => {
                self.picker.move_cursor(-1);
                cx.notify();
                true
            }
            "enter" | "space" => {
                if let Some(edit) = self.picker.toggle_active() {
                    self.apply_picker_edit(edit, cx);
                }
                true
            }
            "escape" => {
                self.picker.close();
                if let Some(cb) = &self.on_close {
                    cb(window, cx);
                }
                cx.notify();
                true
            }
            _ => false,
        }
    }

    /// Toggles the option at `visible_index` and refreshes the picker live.
    pub fn toggle_visible(&mut self, visible_index: usize, cx: &mut Context<Self>) -> bool {
        self.picker
            .toggle_visible(visible_index)
            .is_some_and(|edit| self.apply_picker_edit(edit, cx))
    }

    /// Unlinks the chip at `chip_index` and refreshes the picker live.
    pub fn remove_chip(&mut self, chip_index: usize, cx: &mut Context<Self>) -> bool {
        self.picker
            .remove_chip(chip_index)
            .is_some_and(|edit| self.apply_picker_edit(edit, cx))
    }

    /// Navigates the workbench to the target element represented by a chip.
    pub fn open_chip(&mut self, selection: Selection, window: &mut Window, cx: &mut Context<Self>) {
        let Some(wb) = self.workbench.upgrade() else {
            return;
        };
        let my_id = cx.entity_id();
        wb.update(cx, |w, cx| {
            w.open_target_skipping(selection, Some(my_id), window, cx);
        });
    }

    fn on_capture_key_down(
        &mut self,
        ev: &KeyDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !self.picker.is_open() {
            return;
        }
        let m = ev.keystroke.modifiers;
        if m.control || m.alt || m.platform || m.function {
            return;
        }
        let key = ev.keystroke.key.as_str();
        let intercepted = match key {
            "up" | "down" | "enter" | "escape" => true,
            "space" => !Focusable::focus_handle(self.filter.read(cx), cx).is_focused(window),
            _ => false,
        };
        if intercepted && self.handle_key(key, window, cx) {
            cx.stop_propagation();
        }
    }

    fn render_chips(&self, cx: &mut Context<Self>) -> Option<gpui::Div> {
        let chips = self.picker.chips();
        if chips.is_empty() {
            return None;
        }
        let theme = cx.theme();
        let (chip_bg, border, muted) = (theme.title_bar, theme.border, theme.muted_foreground);
        let items = chips.into_iter().enumerate().map(|(ix, chip)| {
            let selection = chip.selection;
            let vid = SharedString::from(chip.visual_id.clone());
            let label = SharedString::from(chip.label.clone());
            let has_vid = !chip.visual_id.is_empty();
            div()
                .flex()
                .items_center()
                .gap_1()
                .pl_2()
                .pr_1()
                .py_0p5()
                .rounded_sm()
                .bg(chip_bg)
                .border_1()
                .border_color(border)
                .text_xs()
                .child(
                    div()
                        .id(("picker-chip", ix))
                        .flex()
                        .items_center()
                        .gap_1()
                        .cursor_pointer()
                        .when(has_vid, |d| d.child(div().text_color(muted).child(vid)))
                        .child(div().truncate().child(label))
                        .on_click(cx.listener(move |this, _, window, cx| {
                            this.open_chip(selection, window, cx);
                        })),
                )
                .child(
                    Button::new(("picker-chip-remove", ix))
                        .ghost()
                        .xsmall()
                        .icon(IconName::Close)
                        .on_click(cx.listener(move |this, _, _, cx| {
                            this.remove_chip(ix, cx);
                        })),
                )
        });
        Some(div().flex().flex_wrap().gap_1().children(items))
    }

    fn render_options(&self, cx: &mut Context<Self>) -> gpui::Div {
        let theme = cx.theme();
        let (active_bg, muted, primary) =
            (theme.list_active, theme.muted_foreground, theme.primary);
        let window_indices = self.picker.window(WINDOW_CAPACITY);
        if window_indices.is_empty() {
            let empty = SharedString::from(self.catalog.tr("picker-empty"));
            return div().px_2().py_2().text_xs().text_color(muted).child(empty);
        }
        let visible = self.picker.visible();
        let start = window_indices
            .first()
            .and_then(|first| visible.binary_search(first).ok())
            .unwrap_or(0);
        let cursor = self.picker.cursor();
        let options = self.picker.options();
        let rows = window_indices
            .iter()
            .enumerate()
            .filter_map(|(offset, &opt_ix)| {
                let opt = options.get(opt_ix)?;
                let vis_ix = start + offset;
                let is_cursor = vis_ix == cursor;
                let has_vid = !opt.visual_id.is_empty();
                let vid = SharedString::from(opt.visual_id.clone());
                let label = SharedString::from(opt.label.clone());
                Some(
                    div()
                        .id(("picker-option", opt_ix))
                        .flex()
                        .items_center()
                        .gap_2()
                        .px_2()
                        .py_1()
                        .rounded_sm()
                        .cursor_pointer()
                        .when(is_cursor, |d| d.bg(active_bg))
                        .child(
                            div()
                                .w(px(14.))
                                .flex_none()
                                .flex()
                                .items_center()
                                .justify_center()
                                .text_color(primary)
                                .when(opt.linked, |d| d.child(Icon::new(IconName::Check).xsmall())),
                        )
                        .when(has_vid, |d| {
                            d.child(div().flex_none().text_xs().text_color(muted).child(vid))
                        })
                        .child(div().flex_1().truncate().child(label))
                        .on_click(cx.listener(move |this, _, _, cx| {
                            this.toggle_visible(vis_ix, cx);
                        })),
                )
            });
        div().flex().flex_col().gap_0p5().children(rows)
    }
}

impl Render for MultiLinkPickerView {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let chips = self.render_chips(cx);
        let list = self.render_options(cx);
        let theme = cx.theme();
        div()
            .id("multi-link-picker")
            .track_focus(&self.focus_handle)
            .capture_key_down(cx.listener(Self::on_capture_key_down))
            .flex()
            .flex_col()
            .gap_2()
            .p_2()
            .rounded_md()
            .bg(theme.background)
            .border_1()
            .border_color(theme.border)
            .text_sm()
            .text_color(theme.foreground)
            .children(chips)
            .child(Input::new(&self.filter).small())
            .child(list)
    }
}
