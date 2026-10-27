//! The state behind the Library panel (job 057, LIB-02): search, facet
//! narrowing and preview, without any UI toolkit.

use std::sync::Arc;

use aegis_client::library::LibraryView;
use aegis_model::library::LibraryPack;

fn view() -> LibraryView {
    LibraryView::new(Arc::new(LibraryPack::builtin_demo()))
}

#[test]
fn it_lists_every_item_until_asked_and_keeps_the_pick_on_the_same_item() {
    let mut v = view();
    let all = v.rows().len();
    assert_eq!(all, 12);
    v.set_query("flood");
    let rows = v.rows();
    assert!(!rows.is_empty() && rows.len() < all);
    v.pick(Some(0));
    let picked = v.picked_item().expect("an item");
    // Narrowing the list keeps the pick when the item is still there.
    v.set_query("flod");
    assert_eq!(v.picked_item(), Some(picked));
    v.set_query("zzzzqq");
    assert!(v.rows().is_empty());
    assert_eq!(v.picked_item(), None, "a pick that dropped out is cleared");
}

#[test]
fn japanese_text_finds_items() {
    let mut v = view();
    v.set_query("ファームウェア");
    assert!(!v.rows().is_empty());
}

#[test]
fn facet_choices_narrow_the_list_and_toggle_off() {
    let mut v = view();
    let choices = v.facet_choices();
    assert!(choices.len() > 3);
    assert!(choices.iter().all(|c| c.ends_with(')')), "{choices:?}");
    let before = v.rows().len();
    v.toggle_facet(0);
    assert!(v.rows().len() < before);
    assert_eq!(v.filter_text().matches(',').count(), 0);
    assert!(!v.filter_text().is_empty());
    // The first choice is now "on": choosing it again removes it.
    v.toggle_facet(0);
    assert_eq!(v.rows().len(), before);
    v.toggle_facet(0);
    v.clear_filters();
    assert_eq!(v.rows().len(), before);
    assert_eq!(v.filter_text(), "");
    v.toggle_facet(10_000); // out of range: ignored
}

#[test]
fn the_preview_shows_the_outline_notes_and_license() {
    let mut v = view();
    v.set_query("can bus");
    v.pick(Some(0));
    let text = v.preview();
    assert!(text.contains("CC0-1.0"), "{text}");
    assert!(text.contains("OR ") || text.contains("- "), "{text}");
    assert!(view().preview().is_empty(), "nothing picked, nothing shown");
}

#[test]
fn search_is_bounded_by_the_query_length() {
    let mut v = view();
    v.set_query(&"a".repeat(10_000));
    assert!(v.query().chars().count() <= 200);
}
