# Coding Guideline

This document summarizes the key principles and best practices to follow when writing code in this project.
These guidelines are intended to keep the codebase readable, maintainable, and robust over time.

---

## Table of Contents

1. [Locality of Behavior (LoB)](#1-locality-of-behavior-lob)
2. [DRY — Don't Repeat Yourself](#2-dry--dont-repeat-yourself)
3. [SOLID Principles](#3-solid-principles)
4. [KISS — Keep It Simple, Stupid](#4-kiss--keep-it-simple-stupid)
5. [YAGNI — You Aren't Gonna Need It](#5-yagni--you-arent-gonna-need-it)
6. [Separation of Concerns (SoC)](#6-separation-of-concerns-soc)
7. [Law of Demeter (LoD)](#7-law-of-demeter-lod)
8. [Composition over Inheritance](#8-composition-over-inheritance)
9. [Fail Fast](#9-fail-fast)
10. [Single Source of Truth (SSOT)](#10-single-source-of-truth-ssot)
11. [Naming Conventions](#11-naming-conventions)
12. [Error Handling](#12-error-handling)
13. [Comments and Documentation](#13-comments-and-documentation)
14. [Small Increments and Continuous Improvement](#14-small-increments-and-continuous-improvement)

---

## 1. Locality of Behavior (LoB)

> "The behavior of a unit of code should be as obvious as possible from the code itself,
> without requiring the reader to navigate to other files or distant parts of the codebase."

### Intent

Code should be understandable in-place. A reader should be able to grasp what a function or module
does by reading it locally, without having to jump around to understand the full behavior.

### Practices

- Keep logic that belongs together physically close to each other.
- Avoid spreading the implementation of a single concept across many distant files or layers.
- Prefer explicit, inline behavior over implicit behavior driven by distant configuration or global state.
- Minimize indirection: use abstractions only when they clearly reduce complexity, not as a reflex.
- Co-locate tests with (or near) the code they verify when it aids understanding.

### Counter-example (bad)

```rust
// file: config.rs
pub static ENABLE_RETRY: bool = true;

// file: http_client.rs  (far away)
if crate::config::ENABLE_RETRY { ... }  // behavior is invisible at the call site
```

### Example (good)

```rust
fn fetch_with_retry(url: &str, retries: u32) -> Result<Response> { ... }
// The retry behavior is visible right at the function signature.
```

---

## 2. DRY — Don't Repeat Yourself

> "Every piece of knowledge must have a single, unambiguous, authoritative representation
> within a system." — Andy Hunt & Dave Thomas, *The Pragmatic Programmer*

### Intent

Avoid duplicating logic, data, or knowledge. When something needs to change, you should
only have to change it in one place.

### Practices

- Extract repeated logic into a function, method, or module.
- Avoid copy-pasting code blocks; refactor instead.
- Keep configuration and constants defined once and referenced everywhere.
- Use code generation or macros for repetitive structural patterns (with care — see KISS).

### Balance with LoB

DRY and LoB can be in tension. Do not blindly deduplicate at the cost of making behavior
invisible. A small, isolated duplication may be preferable to a premature abstraction that
harms readability. Apply judgment:

- **Duplicated logic** (algorithms, business rules): extract aggressively.
- **Duplicated structure** (similar-looking but semantically distinct code): tolerate with care.

---

## 3. SOLID Principles

SOLID is an acronym for five object-oriented design principles that, together, produce
systems that are easy to maintain and extend.

### S — Single Responsibility Principle (SRP)

> A module, class, or function should have **one reason to change**.

- Each unit of code should do one thing and do it well.
- If a function parses input *and* formats output *and* writes to a file, split it.
- Maps directly to how modules are divided in this project (see `ARCHITECTURE.md`).

### O — Open/Closed Principle (OCP)

> Software entities should be **open for extension**, but **closed for modification**.

- Design modules so that new behavior can be added by extending (e.g. adding a new impl or variant),
  not by editing existing, proven code.
- Prefer trait-based polymorphism over large `if`/`match` chains that must grow with each new case.

### L — Liskov Substitution Principle (LSP)

> Objects of a derived type must be **substitutable** for their base type without breaking correctness.

- An implementation of a trait must honor the full semantic contract of that trait,
  not just its syntactic signature.
- Do not return errors or panic in methods where callers reasonably expect success based on the contract.

### I — Interface Segregation Principle (ISP)

> Clients should not be forced to depend on methods they do not use.

- Prefer small, focused traits over large, monolithic ones.
- A type that needs only `read` should not be forced to implement `write`.
- Split large traits into role-specific ones that can be composed.

### D — Dependency Inversion Principle (DIP)

> High-level modules should not depend on low-level modules. Both should depend on **abstractions**.

- Define interfaces (traits) at the boundary between layers.
- The application core (domain/business logic) should own its abstractions;
  infrastructure code should implement them.
- This makes it possible to swap implementations (e.g. mock transports in tests) without touching
  the core logic.

---

## 4. KISS — Keep It Simple, Stupid

> "Simplicity is the ultimate sophistication." — Leonardo da Vinci

### Intent

Prefer the simplest solution that correctly solves the problem. Complexity is a cost
that must be justified by a clear benefit.

### Practices

- Write the obvious, straightforward implementation first.
- Introduce abstraction only when it earns its place (i.e. it reduces *net* complexity).
- Avoid over-engineering for hypothetical future requirements.
- If a simpler data structure suffices, use it. Reach for complex structures only when needed.
- A function that fits in one screen is easier to reason about than one that does not.

### Warning signs of unnecessary complexity

- More than three levels of indirection for a simple operation.
- Traits or generics added before there are two concrete implementations.
- Configuration flags that control behavior that has only ever been used one way.

---

## 5. YAGNI — You Aren't Gonna Need It

> "Always implement things when you actually need them, never when you just foresee that you need them."
> — Ron Jeffries

### Intent

Do not add functionality until it is actually required. Speculative generality is a form of
technical debt because it must be maintained even if it is never used.

### Practices

- Do not add parameters, options, or branches "just in case".
- Do not abstract prematurely. Wait until you have at least two concrete cases before extracting
  a shared abstraction.
- Delete dead code. Code that is not used is a liability, not an asset.
- Revisit "future-proofing" comments (`// will need this later`) and remove them unless there is
  a concrete near-term plan.

---

## 6. Separation of Concerns (SoC)

> Different aspects of a system should be handled by distinct, non-overlapping modules.

### Intent

Mixing concerns creates code that is harder to test, understand, and change.
Each module should have a clear, single responsibility in the overall system.

### Practices

- Keep I/O (reading files, network, user input) separate from pure logic.
- Keep presentation/rendering separate from domain logic.
- Keep parsing/validation separate from business rule execution.
- In this project: the `dot_renderer` must not know about LSP; the `lsp_client` must not know
  about call graphs. (See `ARCHITECTURE.md` for the full dependency diagram.)

---

## 7. Law of Demeter (LoD)

> "Talk only to your immediate friends." (Principle of Least Knowledge)

### Intent

A method should only call methods on:

1. Itself.
2. Objects passed as arguments.
3. Objects it created directly.
4. Its own fields.

It should **not** reach through an object to call methods on a deeply nested object.

### Example

```rust
// Bad: reaching through multiple levels
order.get_customer().get_address().get_city()

// Good: expose only what callers need
order.customer_city()
```

Violating LoD creates tight coupling: a change deep inside a data structure can break
callers that should not have been aware of that structure.

---

## 8. Composition over Inheritance

> Favor assembling behavior from small, focused components rather than inheriting it
> from a large base class.

### Intent

Inheritance hierarchies tend to become rigid and fragile over time.
Composition allows behavior to be mixed and matched flexibly.

### Practices

- In Rust: prefer trait implementations and struct composition over trait object hierarchies.
- Build complex types by embedding simpler structs.
- Use the "has-a" relationship more often than "is-a".
- When inheritance seems necessary, consider whether a trait with a default implementation
  (plus a wrapper struct) would serve better.

---

## 9. Fail Fast

> Detect and report errors as early and as explicitly as possible.

### Intent

The sooner an error is surfaced, the cheaper it is to diagnose and fix.
Silent failures, default fallbacks, and deferred error reporting all hide bugs.

### Practices

- Validate inputs at the boundary of a module (e.g. CLI argument parsing, deserialization).
- Use `Result` and propagate errors explicitly; do not silently swallow them.
- Prefer returning an `Err` over returning a sentinel value (e.g. `-1`, `""`, `null`).
- Prefer `expect("message")` over silent `unwrap()` in places where a panic is truly
  a programming error, and add a descriptive message explaining the invariant.
- Assertions and invariant checks are valuable documentation as well as guards.

---

## 10. Single Source of Truth (SSOT)

> Every piece of data or configuration should be stored in exactly one place.

### Intent

When the same fact exists in multiple places, they inevitably drift out of sync.
One authoritative source eliminates that class of bug.

### Practices

- Define constants, enums, and configuration values once; reference them everywhere.
- Derive secondary data from primary data rather than storing both independently.
- Avoid maintaining parallel data structures that must be kept in sync manually.
- Schema or type definitions should be the source of truth; do not duplicate their
  logic in comments or documentation.

---

## 11. Naming Conventions

Good names are the cheapest form of documentation.

### General Rules

- Names should reveal **intent**, not implementation details.
  - Prefer `elapsed_seconds` over `n` or `val`.
- Avoid abbreviations unless they are universally understood in the domain (e.g. `url`, `id`).
- Boolean variables and functions should read as predicates: `is_empty`, `has_children`, `can_retry`.
- Functions that perform actions should use verbs: `parse_config`, `send_request`, `build_graph`.
- Use consistent vocabulary across layers (see `ARCHITECTURE.md` Naming Policy).

### Layer-Specific Naming

| Layer | Vocabulary |
|---|---|
| Transport (I/O) | `read`, `write`, `frame`, `byte` |
| Protocol (message routing) | `send`, `receive`, `request`, `response`, `notification` |
| Domain / Application | `build`, `render`, `analyze`, `symbol`, `call`, `graph` |

### Avoid

- Names that differ only by a number: `handler1`, `handler2`.
- Generic names that carry no meaning: `data`, `info`, `manager`, `util`.
- Misleading names: a function called `get_user` should not create a user as a side effect.

---

## 12. Error Handling

### Principles

- **Be explicit**: use `Result<T, E>` with a meaningful error type; do not return `Option`
  when an error reason matters.
- **Be specific**: define error variants that carry enough context to diagnose the problem.
- **Be consistent**: follow a uniform error-naming convention (see `ARCHITECTURE.md`
  Error naming section for the `transport:*` / `protocol:*` / `timeout:*` prefix policy).
- **Do not swallow errors**: `let _ = some_fallible_call();` is almost always wrong.
- **Log at the right level**: log errors where they are *handled*, not where they are *propagated*.

### Error Message Quality

An error message should answer:
1. **What** went wrong.
2. **Where** (which module, operation, or resource).
3. **Why**, if known.

```
// Bad
"failed"

// Good
"transport: failed to read frame header: unexpected EOF after 3 bytes"
```

---

## 13. Comments and Documentation

### When to Comment

- **Why**, not what: the code already says *what* it does; comments should explain *why*.
- Non-obvious invariants, preconditions, or postconditions.
- Deliberate trade-offs or workarounds (include a link to an issue or ticket if possible).
- Public API: every exported function, type, and module should have a doc-comment.

### When NOT to Comment

- Restating the code in prose: `// increment i by 1` above `i += 1`.
- Commented-out code: delete it; version control preserves history.
- TODO comments left indefinitely: convert to a tracked issue or remove.

### Doc-Comment Style (Rust)

```rust
/// Returns the shortest path between `src` and `dst` in the call graph.
///
/// # Errors
///
/// Returns `Err` if either node does not exist in the graph.
pub fn shortest_path(&self, src: NodeId, dst: NodeId) -> Result<Vec<NodeId>> { ... }
```

---

## 14. Small Increments and Continuous Improvement

### Work in Small Steps

- Make changes in small, reviewable increments rather than large, sweeping rewrites.
- Each commit or pull request should represent a single logical change.
- A change that mixes refactoring with new features is harder to review and harder to revert.

### Avoid Accumulating Technical Debt

- Fix a known code smell when you are working in that area, even if it was not the original task.
- Leave the code cleaner than you found it (Boy Scout Rule).
- Do not suppress warnings without justification; warnings are early indicators of debt.
- Refactor proactively before a module grows too large to understand in isolation.

### Code Review Mindset

- Review code as if you will be the one maintaining it in a year.
- Question complexity: ask "is there a simpler way?" before approving clever solutions.
- Treat review comments as discussions, not verdicts.

---

## Summary Table

| Principle | Core Question to Ask |
|---|---|
| **Locality of Behavior** | Can I understand this code without jumping to another file? |
| **DRY** | Is this logic defined in exactly one place? |
| **SRP** | Does this unit have exactly one reason to change? |
| **OCP** | Can I extend this without editing existing code? |
| **LSP** | Does this implementation fully honor the contract it claims? |
| **ISP** | Am I forcing callers to depend on things they do not need? |
| **DIP** | Does high-level logic depend on abstractions, not concrete details? |
| **KISS** | Is there a simpler way to achieve the same result? |
| **YAGNI** | Do I need this right now, or am I speculating about the future? |
| **SoC** | Are distinct concerns handled by distinct modules? |
| **LoD** | Am I reaching through objects to call distant methods? |
| **Composition** | Am I building behavior from small pieces rather than inheriting it? |
| **Fail Fast** | Will errors surface immediately and explicitly? |
| **SSOT** | Is every fact stored and maintained in exactly one place? |
