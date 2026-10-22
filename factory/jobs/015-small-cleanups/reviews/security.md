VERDICT: PASS

## Security review: `015-small-cleanups`

- `ModelError` continues to exclude user-supplied entity names and malformed input strings from both `Display` and `Debug`:
  - `EntityKind` is a fixed compile-time enum.
  - `CatalogSyntax` comes from the validated methodology definition, never from the rejected input string.
  - `CatalogRef` holds bounded numeric `u32` segments (`[u32; 4]`, `len <= 4`), never unparsed input text.
- Tested by `errors_never_contain_names` and `specific_model_error_messages`.
