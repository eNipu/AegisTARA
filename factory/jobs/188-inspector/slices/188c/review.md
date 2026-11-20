VERDICT: PASS

For the owner:
- Wired `"go-to-cause"` (`GoToCause` action, shortcut, menu entry, and `Workbench::go_to_cause` / `InspectorPanel::go_to_cause`) and `aegis_client::inspector::inspector_view` into `InspectorPanel`.
- `InspectorPanel` now renders the header (system usage, provenance, exclusion badge and clickable exclusion causes, description), summary fields with tone pills, the Explain-why section with a "Go to cause" action button and clickable cause chips, forward relation rows (with inline `MultiLinkPickerView`), and reverse trace-spine neighbour rows ("Used by").
- Navigating to `Selection::TreeNode(tree_id, node_id)` reveals and opens the owning `Selection::Tree(tree_id)` tab in the center dock while keeping `InspectorPanel` focused on the specific step or gate node without double-leasing GPUI entities.

Findings:
- None.

Lesson: none

NEXT: gate
