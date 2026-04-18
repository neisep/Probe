# Defensive Programming Rules

- Validate all inputs at module boundaries.
- Never unwrap() in core/ or infra/.
- Use Result<T, E> everywhere.
- Handle unexpected states explicitly.
- Prefer small, pure functions.
- Fail safe, not fast.

