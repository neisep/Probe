# OpenAPI Import — Task List

Feature: OpenAPI 2.x + 3.x import with re-import merge (no clobber).
See plan: ~/.claude/plans/zippy-spinning-micali.md

## Tasks

- [ ] 1. Add `serde_yaml = "0.9"` and `openapiv3 = "2"` to `Cargo.toml`
- [ ] 2. Extend `RequestDraft` with `import_key: Option<String>` — update all struct literals + serde round-trip test
- [ ] 3. Extend `.http` parser + writer with `# @probe-import-key` directive + tests
- [ ] 4. Create `src/openapi/parser.rs` — detect 2.x/3.x, parse → `Vec<ImportedOperation>`, resolve base URL; unit test with inline Petstore fixture
- [ ] 5. Create `src/openapi/merge.rs` — `compute_merge()` + `MergePreview`; tests: all-new, all-updated, unchanged, hand-crafted preserved
- [ ] 6. Create `src/openapi/source.rs` — `fetch_url()` via spawned thread + one-shot tokio runtime
- [ ] 7. Wire into `app.rs` — `PendingOpenApiImport`, file/URL import, preview dialog (new/updated/unchanged counts)
- [ ] 8. Security scheme → auth hint mapping in parser (Bearer / Basic / ApiKey, no credentials)
- [ ] 9. Integration tests — Petstore 2.x + 3.x JSON: assert operation count, spot-check import_key/name/folder/url
- [ ] 10. Re-import idempotency test — `compute_merge` twice → `new=0, updated=0, unchanged=N`

## Merge rules (quick reference)

| Field | On re-import |
|---|---|
| `name`, `folder`, `url`, `query_params` | Updated from spec |
| `auth`, `headers`, `body`, `attach_oauth` | Preserved (user owns) |
| Hand-crafted requests (no `import_key`) | Always preserved |
