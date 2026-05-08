# Language Completeness Audit

Compare `docs/language/boon-language-manifest.toml` with the parser, HIR,
shape/type checker, semantic IR, DD lowering, generated code, runtime host, and
tests.

List every accepted syntax or semantic feature that lacks any required coverage
layer:

- parser
- resolver
- shape/type checker
- semantic IR
- DD graph IR/lowering
- generated runtime
- terminal/native/browser host parity
- positive fixture
- negative diagnostic fixture

Return the required prompt audit JSON schema. Use `fail` if any accepted feature
lacks complete coverage.
