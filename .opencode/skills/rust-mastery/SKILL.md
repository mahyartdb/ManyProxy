---
name: rust-mastery
description: |
  Rust idioms and stdlib overview plus router to specialists. Use for general "the Rust way" questions and Vec/String/HashMap/iterator stdlib help. Defer: ownership and borrow-checker errors to rust-ownership, Result/Option handling to rust-error-handling, newtype/typestate design to rust-type-system and rust-patterns, async and threads to rust-async. Never claims other rust-* jobs.
---

# Rust Mastery

Router plus stdlib overview for writing idiomatic, safe, and performant Rust code. This skill synthesizes best practices from "The Rust Programming Language", "Programming Rust", "Effective Rust", and community patterns.

## Quick Navigation

Read additional references based on the task:

- **references/ownership.md** - Deep dive into ownership, borrowing, and lifetimes
- **references/traits.md** - Trait design, standard traits, and generics
- **references/error-handling.md** - Result, Option, and error handling patterns
- **references/idioms.md** - Common Rust idioms and patterns

## Core Principles

### 1. Embrace the Type System

Rust's type system is your ally. Use it to:

- **Express invariants at compile time** - If a state shouldn't exist, make it unrepresentable
- **Encode business logic in types** - Use newtypes, enums, and marker types
- **Leverage zero-cost abstractions** - Generics and traits have no runtime overhead

```rust
// Bad: String that could be anything
fn process_email(email: String) { ... }

// Good: Newtype enforces validation
struct Email(String);
impl Email {
    pub fn new(s: &str) -> Result<Self, EmailError> {
        // Validation happens once, at creation
        if s.contains('@') && s.contains('.') {
            Ok(Self(s.to_string()))
        } else {
            Err(EmailError::Invalid)
        }
    }
}
fn process_email(email: Email) { ... } // Can only receive valid emails
```

### 2. Ownership (summary - depth in `rust-ownership`)

One owner per value, dropped at scope end; moves transfer, borrows reference. One mutable XOR many immutable references; references always valid. Borrow-checker errors ("cannot borrow as mutable", "value moved", "missing lifetime") go to `rust-ownership`.

### 3. Option/Result and errors (summary - depth in `rust-error-handling`)

Prefer transforms (`map`, `?`) over explicit `match`. `Result` for recoverable failure (thiserror/anyhow plus context); `panic!` for bugs and tests. Custom error design goes to `rust-error-handling`.

### 4. Iterator Patterns

Prefer iterator transforms over explicit loops. They're often more readable and equally performant.

```rust
// Imperative style
let mut results = Vec::new();
for item in items {
    if item.is_valid() {
        results.push(item.transform());
    }
}

// Functional style (preferred)
let results: Vec<_> = items
    .iter()
    .filter(|item| item.is_valid())
    .map(|item| item.transform())
    .collect();

// Useful iterator methods
items.iter().find(|x| x.id == target_id)     // First match
items.iter().any(|x| x.is_active())          // Existence check
items.iter().all(|x| x.is_valid())           // Universal check
items.iter().fold(0, |acc, x| acc + x.value) // Reduce/accumulate
items.iter().flat_map(|x| x.children())      // Flatten nested
items.iter().enumerate()                      // Index + value
items.iter().zip(other.iter())               // Pair two iterators
```

### 5. Struct and Enum Design

**Structs for data with named fields:**

```rust
struct User {
    id: UserId,
    name: String,
    email: Email,
    created_at: DateTime<Utc>,
}
```

**Enums for variants/states:**

```rust
enum ConnectionState {
    Disconnected,
    Connecting { attempt: u32 },
    Connected { session_id: String },
    Error { message: String, retryable: bool },
}

// Pattern matching extracts data
match state {
    ConnectionState::Connected { session_id } => {
        println!("Session: {}", session_id);
    }
    ConnectionState::Error { message, retryable: true } => {
        println!("Retryable error: {}", message);
    }
    _ => {}
}
```

### 6. Common Patterns

**Builder Pattern for complex construction:**

```rust
#[derive(Default)]
struct RequestBuilder {
    url: Option<String>,
    method: Method,
    headers: HashMap<String, String>,
    timeout: Duration,
}

impl RequestBuilder {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn url(mut self, url: impl Into<String>) -> Self {
        self.url = Some(url.into());
        self
    }

    pub fn header(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.headers.insert(key.into(), value.into());
        self
    }

    pub fn build(self) -> Result<Request, BuildError> {
        let url = self.url.ok_or(BuildError::MissingUrl)?;
        Ok(Request { url, method: self.method, headers: self.headers, timeout: self.timeout })
    }
}

// Usage
let request = RequestBuilder::new()
    .url("https://api.example.com")
    .header("Authorization", "Bearer token")
    .build()?;
```

**Newtype Pattern for type safety:**

```rust
struct UserId(u64);
struct OrderId(u64);

// These cannot be confused, even though both are u64
fn get_user_orders(user_id: UserId) -> Vec<OrderId> { ... }
```

**RAII with Drop:**

```rust
struct TempFile {
    path: PathBuf,
}

impl Drop for TempFile {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.path);
    }
}
// File is automatically deleted when TempFile goes out of scope
```

### 7. Performance Guidelines

- **Clone judiciously** - Cloning is explicit in Rust; use `&str` instead of `String` where possible
- **Use `Cow<str>`** - For functions that might or might not need to allocate
- **Prefer `&[T]`** - Over `&Vec<T>` in function parameters
- **Use `impl Trait`** - For returning iterators without boxing

```rust
use std::borrow::Cow;

// Accepts both owned and borrowed strings efficiently
fn process(input: Cow<'_, str>) -> String {
    if input.contains("special") {
        input.to_uppercase() // Only allocates when needed
    } else {
        input.into_owned()
    }
}

// Flexible slice parameter
fn sum(numbers: &[i32]) -> i32 {
    numbers.iter().sum()
}
// Works with Vec, array, or any contiguous sequence
sum(&vec![1, 2, 3]);
sum(&[1, 2, 3]);
```

### 8. Common Compilation Errors

**E0382: Use of moved value**

```rust
// Error: s moved to s2, then used
let s = String::from("hello");
let s2 = s;
println!("{}", s); // Error!

// Fix: Clone if you need both, or use references
let s = String::from("hello");
let s2 = s.clone();
println!("{} {}", s, s2);
```

**E0502: Cannot borrow as mutable because also borrowed as immutable**

```rust
// Error: Immutable and mutable borrow overlap
let mut v = vec![1, 2, 3];
let first = &v[0];
v.push(4); // Error: v borrowed mutably while first exists
println!("{}", first);

// Fix: End immutable borrow before mutable borrow
let mut v = vec![1, 2, 3];
let first = v[0]; // Copy the value, no borrow
v.push(4);
println!("{}", first);
```

**E0106: Missing lifetime specifier**

```rust
// Error: Rust can't infer lifetime
fn longest(x: &str, y: &str) -> &str { // Error!
    if x.len() > y.len() { x } else { y }
}

// Fix: Add lifetime annotation
fn longest<'a>(x: &'a str, y: &'a str) -> &'a str {
    if x.len() > y.len() { x } else { y }
}
```

## Standard Traits to Know

| Trait              | Purpose                    | When to Implement               |
| ------------------ | -------------------------- | ------------------------------- |
| `Debug`            | Debug formatting (`{:?}`)  | Always (use `#[derive(Debug)]`) |
| `Clone`            | Explicit duplication       | When copying makes sense        |
| `Copy`             | Implicit copy (bitwise)    | Small, stack-only types         |
| `Default`          | Default value              | When there's a sensible default |
| `PartialEq`/`Eq`   | Equality comparison        | Comparable types                |
| `PartialOrd`/`Ord` | Ordering                   | Sortable types                  |
| `Hash`             | Hashing                    | HashMap/HashSet keys            |
| `Display`          | User-facing format         | Public types                    |
| `From`/`Into`      | Type conversion            | When conversion is natural      |
| `AsRef`/`AsMut`    | Cheap reference conversion | Flexible APIs                   |
| `Deref`            | Smart pointer behavior     | Wrapper types                   |
| `Iterator`         | Iteration                  | Custom collections              |
| `Drop`             | Cleanup on scope exit      | RAII resources                  |

## Crates to Know

| Category       | Crate       | Purpose                          |
| -------------- | ----------- | -------------------------------- |
| Error Handling | `thiserror` | Derive Error for library types   |
| Error Handling | `anyhow`    | Flexible errors for applications |
| Serialization  | `serde`     | Serialize/deserialize anything   |
| Async Runtime  | `tokio`     | Async runtime with full features |
| HTTP Client    | `reqwest`   | Ergonomic HTTP client            |
| CLI            | `clap`      | Command-line argument parsing    |
| Logging        | `tracing`   | Structured logging/tracing       |
| Testing        | `proptest`  | Property-based testing           |
| Testing        | `criterion` | Benchmarking                     |

## Quick Reference

See `references/` for deep dives:

- **ownership.md** - Ownership, borrowing, lifetimes in depth
- **traits.md** - Trait system, generics, trait objects
- **error-handling.md** - Complete error handling guide
- **idioms.md** - Idiomatic patterns and anti-patterns

## Review Checklist

- Prefer borrowing in function parameters unless ownership is required.
- Make invalid states unrepresentable with enums, newtypes, or typed builders.
- Use `Result` for expected failures and reserve panics for bugs/invariants.
- Derive standard traits where they improve debugging, testing, or ergonomics.
- Keep lifetimes tied to real borrowed data; do not add annotations as decoration.
- Prefer iterator adapters when they clarify ownership and control flow.

## References

- [The Rust Programming Language](https://doc.rust-lang.org/book/)
- [Rust Reference](https://doc.rust-lang.org/reference/)
- [Rust by Example](https://doc.rust-lang.org/rust-by-example/)
- [std documentation](https://doc.rust-lang.org/std/)
