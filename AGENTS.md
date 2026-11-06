# Repository rules

- Keep one library crate, `crates/aclp`, plus the `crates/xtask` task runner. Protocol definitions belong in `superaligned-to/aclp`.
- Generate `crates/aclp/src/events.rs` with `cargo xtask generate`; do not edit it by hand. Update `protocol-source.json` and `crates/aclp/schema.json` together.
- Keep behaviour at parity with `superaligned-to/aclp-ts`; share fixtures rather than copying expectations by hand. Run `cargo xtask interop <aclp-ts checkout>` (built with `pnpm build`) after changing wire behaviour.
- Keep callbacks application-neutral. Authentication uses configured HTTP credentials; OAuth and harness adapters are outside this SDK.
- Run `cargo xtask check` before committing. Put unit tests beside the code and public-API tests in `crates/aclp/tests`.
- Use one-line conventional commits.
- Keep the guard's dependency reasons, build-script list and module ceiling current. Preserve blank lines between logical blocks.
