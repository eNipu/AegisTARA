
## Rework 162b
Owner: fix in the reader (option a). Sort tree files by the number in the file name (not by string), and fill in a missing `visual_id` from the file name. Do not change the 162a writer. Add a test with more than 100 trees that compares `save()` bytes after a round trip. Also fix the smaller findings in slices/162b/review.md: replace the linear repeated-path search with a `BTreeSet`, use `wire::FORMAT_VERSION` instead of the literal `1`, and make `file_order_does_not_matter` compare `save()` bytes. Leave the Project::load error-path question for 162c.
