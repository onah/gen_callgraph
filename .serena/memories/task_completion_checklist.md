# After-task checklist
1. Ensure architecture alignment (layer boundaries and dependency direction).
2. Run formatting/lint/tests: `./check.sh`.
3. Keep `main.rs` thin and avoid adding domain logic to transport internals.
4. Prefer meaningful errors for retryable readiness/indexing states.
