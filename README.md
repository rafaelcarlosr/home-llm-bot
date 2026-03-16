# home-llm-bot

A local Telegram bot that orchestrates Home Assistant and LM Studio with speech-to-text. Everything runs locally — no cloud dependencies. The LLM decides what actions to take via function calling.

## Architecture

```
┌──────────────┐     ┌──────────────────────────────────────┐
│  Telegram     │────▶│  home-llm-bot (Rust)                 │
│  (you/family) │◀────│                                      │
└──────────────┘     │  ┌─────────┐  ┌────────────────────┐ │
                     │  │Telegram │  │   Orchestrator      │ │
                     │  │Handler  │──│   (function calling │ │
                     │  └─────────┘  │    loop)            │ │
                     │               └──────┬─────────────┘ │
                     │                      │               │
                     │          ┌───────────┼──────────┐    │
                     │          ▼           ▼          ▼    │
                     │  ┌──────────┐ ┌──────────┐ ┌──────┐ │
                     │  │LM Studio │ │Home Asst.│ │SQLite│ │
                     │  │(local)   │ │(local)   │ │(state│ │
                     │  └──────────┘ └──────────┘ └──────┘ │
                     └──────────────────────────────────────┘
                              │
                     ┌────────▼────────┐
                     │  Whisper (local) │
                     │  speech-to-text  │
                     └─────────────────┘
```

- **Bot**: Rust service running in LXC container (Proxmox) or locally
- **LM Studio**: Local LLM (e.g. Qwen 2.5 7B) on desktop/remote — OpenAI-compatible API
- **Home Assistant**: Home automation hub on local network
- **Whisper**: Local speech-to-text service in separate container

## Setup

### Prerequisites

- LM Studio running with a model loaded
- Home Assistant instance
- Whisper service (local or remote)
- Telegram bot token from BotFather

### Local Development

1. Clone repo and create `.env`:

```bash
cp .env.example .env
# Edit .env with your values
```

2. Run with docker-compose:

```bash
docker-compose up --build
```

3. Or run locally with Rust:

```bash
cargo run
```

### Production (Proxmox LXC)

1. Create LXC container from Debian image
2. Install Rust and dependencies
3. Copy repo and run:

```bash
cargo build --release
./target/release/home-llm-bot
```

Or use the Dockerfile to create an image.

## Environment Variables

See `.env.example` for all required variables.

## Features

- Telegram interface with stateful conversations
- Function calling with local LM Studio
- Home Assistant integration (lights, thermostat, entity state)
- Speech-to-text with Whisper
- Shared family conversation context with SQLite persistence
- Plugin system for easy extensions

## Project Structure

```
src/
├── main.rs                  # Entry point — wires everything together
├── lib.rs                   # Library crate — exposes all modules
├── config.rs                # Environment variable configuration
├── error.rs                 # Custom error types (BotError enum)
├── state.rs                 # Conversation state + SQLite persistence
├── orchestrator.rs          # LLM function-calling loop
├── telegram.rs              # Telegram bot dispatcher (text + voice)
└── plugins/
    ├── mod.rs               # Plugin trait + PluginRegistry
    ├── home_assistant.rs    # Home Assistant REST API (5 functions)
    ├── lm_studio.rs         # LM Studio OpenAI-compatible provider
    └── whisper.rs           # Whisper speech-to-text provider
```

## Testing

```bash
# Unit tests (no external services needed)
cargo test

# Integration tests (requires running LM Studio, HA, Whisper)
cargo test -- --ignored
```

---

## Rust for Java Developers

This project was built as a learning exercise by a developer with 20+ years of Java experience. Every concept below is explained through the lens of Java equivalents, with real code from this project.

### Table of Contents

1. [Ownership and Move Semantics](#1-ownership-and-move-semantics)
2. [References and Borrowing](#2-references-and-borrowing)
3. [The `String` vs `&str` Duality](#3-the-string-vs-str-duality)
4. [Structs and `impl` Blocks (Classes Without Inheritance)](#4-structs-and-impl-blocks-classes-without-inheritance)
5. [Traits (Interfaces, but Better)](#5-traits-interfaces-but-better)
6. [`Option<T>` — Null Safety at Compile Time](#6-optiont--null-safety-at-compile-time)
7. [`Result<T, E>` and the `?` Operator — Checked Exceptions Done Right](#7-resultt-e-and-the--operator--checked-exceptions-done-right)
8. [Custom Error Types with `thiserror`](#8-custom-error-types-with-thiserror)
9. [The `match` Expression — Exhaustive Pattern Matching](#9-the-match-expression--exhaustive-pattern-matching)
10. [`Box<dyn Trait>` — Dynamic Dispatch (Interface References)](#10-boxdyn-trait--dynamic-dispatch-interface-references)
11. [`Arc<T>` — Thread-Safe Shared Ownership](#11-arct--thread-safe-shared-ownership)
12. [`Mutex<T>` — Interior Mutability with Locking](#12-mutext--interior-mutability-with-locking)
13. [`Arc<Mutex<T>>` — Shared Mutable State Across Tasks](#13-arcmutext--shared-mutable-state-across-tasks)
14. [`async`/`await` and Tokio](#14-asyncawait-and-tokio)
15. [`move` Closures — Capturing Ownership](#15-move-closures--capturing-ownership)
16. [Iterator Chains and `.collect()`](#16-iterator-chains-and-collect)
17. [The Module System and Crate Structure](#17-the-module-system-and-crate-structure)
18. [Derive Macros — Auto-Implementing Traits](#18-derive-macros--auto-implementing-traits)
19. [The Builder Pattern (Consuming `self`)](#19-the-builder-pattern-consuming-self)
20. [Testing](#20-testing)

---

### 1. Ownership and Move Semantics

**Java:** Every object lives on the heap, managed by the GC. You pass references freely — multiple variables can point to the same object. The GC cleans up when no references remain.

**Rust:** Every value has exactly one owner. When you assign a value to another variable or pass it to a function, ownership **moves** — the original variable becomes invalid.

```rust
// From main.rs — ownership transfer into Arc
let lm_provider = LMStudioProvider::new(config.lm_studio_url.clone());
let orchestrator = Arc::new(Orchestrator::new(lm_provider, registry, model));
// `lm_provider` and `registry` have MOVED into Orchestrator.
// Using `lm_provider` here would be a compile error.
```

**Java equivalent:**
```java
var lmProvider = new LMStudioProvider(config.lmStudioUrl);
var orchestrator = new Orchestrator(lmProvider, registry, model);
// In Java, `lmProvider` is still usable — both variables point to the same object.
// In Rust, `lm_provider` is consumed. Only `orchestrator` owns it now.
```

**Why this matters:** No garbage collector needed. Memory is freed deterministically when the owner goes out of scope. No GC pauses, no finalizers, no `WeakReference` headaches. The compiler tracks it all at compile time.

**When you need a copy:** Use `.clone()` to make an explicit deep copy. Unlike Java's `clone()` debacle, Rust's `Clone` trait is straightforward and opt-in.

```rust
// From main.rs — we need the URL string in two places
let lm_provider = LMStudioProvider::new(config.lm_studio_url.clone());
// .clone() makes a full copy of the String. Explicit, visible, intentional.
```

---

### 2. References and Borrowing

**Java:** All object access is through references. You never think about whether you're reading or writing — the JVM handles it.

**Rust:** You can *borrow* a value without taking ownership. Two flavors:

| Rust | Java Equivalent | Rule |
|------|-----------------|------|
| `&T` (shared reference) | Read-only parameter | Any number of readers, OR... |
| `&mut T` (mutable reference) | Parameter you modify | ...exactly one writer. Never both. |

```rust
// From orchestrator.rs — &self borrows Orchestrator immutably, &mut state borrows mutably
pub async fn process_message(
    &self,                          // shared borrow — we only read the orchestrator's config
    user_message: &str,             // shared borrow — we only read the message
    state: &mut ConversationState,  // exclusive mutable borrow — we modify state
) -> Result<String> {
    state.add_message("user", user_message, None); // OK: we have &mut state
    // ...
}
```

**Java equivalent:**
```java
public String processMessage(String userMessage, ConversationState state) {
    state.addMessage("user", userMessage, null); // Java doesn't prevent concurrent mutation
}
```

**The key difference:** In Java, nothing stops two threads from calling `state.addMessage()` simultaneously, leading to a race condition. In Rust, if you have `&mut state`, the compiler guarantees no one else can access `state` — at compile time. Not with a lock. Not with a `volatile`. The borrow checker simply won't let the program compile.

---

### 3. The `String` vs `&str` Duality

This confuses every Java developer at first. Java has one string type (`String`). Rust has two:

| Type | Ownership | Java Equivalent | When to Use |
|------|-----------|-----------------|-------------|
| `String` | Owned, heap-allocated, growable | `String` | When you need to own/store the string |
| `&str` | Borrowed slice, read-only view | `String` passed as a parameter | When you only need to read it |

```rust
// From config.rs — Config OWNS its strings (they live as long as Config does)
pub struct Config {
    pub telegram_token: String,     // Owned: Config is responsible for this memory
    pub lm_studio_url: String,
}

// From orchestrator.rs — functions BORROW strings they only need to read
pub async fn call_llm(
    &self,
    messages: Vec<Value>,
    tools: Vec<Value>,
    model: &str,                    // Borrowed: we just read it, caller keeps ownership
) -> Result<Value> { /* ... */ }
```

**Mental model:** Think of `String` as a `StringBuilder` that you own, and `&str` as a read-only view into any string data. When a function takes `&str`, it accepts both `String` (via auto-deref) and string literals.

**The conversions you'll write constantly:**

```rust
"hello"                  // &str (string literal, lives in binary)
"hello".to_string()      // String (allocates on heap)
my_string.as_str()       // &str (borrows from String)
&my_string               // &str (auto-deref, same as .as_str())
format!("hi {}", name)   // String (always allocates)
```

---

### 4. Structs and `impl` Blocks (Classes Without Inheritance)

**Java:** Classes bundle data and behavior. Inheritance creates is-a hierarchies.

**Rust:** `struct` holds data. `impl` blocks add methods. No inheritance — composition and traits instead.

```rust
// From plugins/lm_studio.rs — struct = data, impl = behavior
pub struct LMStudioProvider {
    url: String,          // Private by default (like Java's package-private)
    client: Client,
}

impl LMStudioProvider {
    // Associated function (Java: static method) — called as LMStudioProvider::new()
    pub fn new(url: String) -> Self {
        Self {
            url,            // Field init shorthand: same as `url: url`
            client: Client::new(),
        }
    }

    // Method — takes &self (Java: instance method)
    pub async fn call_llm(&self, messages: Vec<Value>, tools: Vec<Value>, model: &str) -> Result<Value> {
        // self.url, self.client accessible here
    }
}
```

**Java equivalent:**
```java
public class LMStudioProvider {
    private final String url;           // final = Rust's default immutability
    private final HttpClient client;

    public LMStudioProvider(String url) {
        this.url = url;
        this.client = HttpClient.newHttpClient();
    }

    public CompletableFuture<JsonNode> callLlm(List<JsonNode> messages, List<JsonNode> tools, String model) {
        // ...
    }
}
```

**Key differences:**
- No `this` keyword — use `self`, `&self`, or `&mut self`
- `Self` (capital S) = the type being implemented (like Java's class name in a static context)
- No constructors — `new()` is just a convention, not language syntax
- You can have **multiple `impl` blocks** for the same struct (useful for organizing code)
- Fields are private by default; use `pub` for public access

---

### 5. Traits (Interfaces, but Better)

**Java:** Interfaces define contracts. Classes implement them. Since Java 8, interfaces can have default methods.

**Rust:** Traits define contracts. Structs implement them. But traits are more powerful — they control operator overloading, automatic behavior (via derive), and can be used as type bounds.

```rust
// From plugins/mod.rs — the Plugin trait (= Java interface)
#[async_trait::async_trait]
pub trait Plugin: Send + Sync {
    async fn execute(&self, function_name: &str, params: Value) -> Result<Value>;
    fn available_functions(&self) -> Vec<FunctionDef>;
}

// From plugins/home_assistant.rs — implementing the trait (= Java implements)
#[async_trait::async_trait]
impl Plugin for HomeAssistantPlugin {
    async fn execute(&self, function_name: &str, params: Value) -> Result<Value> {
        match function_name {
            "turn_on_light" => { /* ... */ }
            "turn_off_light" => { /* ... */ }
            _ => Err(BotError::HomeAssistant(format!("Unknown function: {}", function_name))),
        }
    }

    fn available_functions(&self) -> Vec<FunctionDef> {
        vec![ /* ... 5 function definitions ... */ ]
    }
}
```

**Java equivalent:**
```java
public interface Plugin {
    CompletableFuture<JsonNode> execute(String functionName, JsonNode params);
    List<FunctionDef> availableFunctions();
}

public class HomeAssistantPlugin implements Plugin {
    @Override
    public CompletableFuture<JsonNode> execute(String functionName, JsonNode params) {
        return switch (functionName) {
            case "turn_on_light" -> /* ... */;
            default -> CompletableFuture.failedFuture(new IllegalArgumentException("Unknown: " + functionName));
        };
    }
}
```

**The `Send + Sync` bounds:** These are marker traits that tell the compiler this trait object is safe to send between threads (`Send`) and safe to share between threads (`Sync`). Java doesn't need these because the JMM handles thread safety differently (often unsafely). In Rust, if your trait object isn't `Send + Sync`, the compiler won't let you use it in `Arc` or pass it across async task boundaries.

**`#[async_trait]`:** Rust doesn't natively support `async fn` in traits yet (it's stabilizing). The `async_trait` macro desugars it into a `Pin<Box<dyn Future>>` return type. You'll see this in any crate that needs async trait methods.

---

### 6. `Option<T>` — Null Safety at Compile Time

**Java:** Any reference can be `null`. `NullPointerException` is the #1 runtime error in Java history. `Optional<T>` exists since Java 8 but is optional (ironic) and rarely used for fields.

**Rust:** There is no `null`. If a value might be absent, you use `Option<T>`:

```rust
// From state.rs — the pool might not exist (tests vs production)
pub struct ConversationState {
    pub family_id: i64,
    pub messages: Vec<Message>,
    pool: Option<SqlitePool>,    // Some(pool) or None
}

// You MUST handle both cases — the compiler won't let you access the inner value directly
pub async fn add_message_persisted(&mut self, role: &str, content: &str, sender_name: Option<String>) -> Result<()> {
    self.add_message(role, content, sender_name.clone());

    if let Some(pool) = &self.pool {    // Only runs if pool is Some
        // ... use pool to persist ...
    }
    // If pool is None, we skip persistence silently. Compiler enforced.
    Ok(())
}
```

**Java equivalent:**
```java
private @Nullable SqlitePool pool;  // Might be null

public void addMessagePersisted(String role, String content, @Nullable String senderName) {
    addMessage(role, content, senderName);
    if (pool != null) {  // You might forget this check. Java compiles either way.
        // ... use pool ...
    }
}
```

**Common `Option` methods (mapped to Java):**

| Rust | Java | What it does |
|------|------|--------------|
| `opt.unwrap()` | `opt.get()` | Get value or panic (crash) |
| `opt.unwrap_or(default)` | `opt.orElse(default)` | Get value or use default |
| `opt.map(\|v\| ...)` | `opt.map(v -> ...)` | Transform the inner value |
| `opt.and_then(\|v\| ...)` | `opt.flatMap(v -> ...)` | Chain operations that return Option |
| `opt.ok_or(err)` | — | Convert Option to Result |
| `if let Some(v) = opt` | `if (opt != null)` | Conditional unwrap |

```rust
// From telegram.rs — chaining Option operations to extract username
let sender = msg
    .from                                    // Option<User>
    .as_ref()                                // Option<&User> (borrow, don't move)
    .and_then(|u| u.username.as_deref())     // Option<&str>
    .unwrap_or("unknown")                    // &str (fallback)
    .to_string();                            // String (owned copy)
```

---

### 7. `Result<T, E>` and the `?` Operator — Checked Exceptions Done Right

**Java:** Checked exceptions are verbose (`try/catch/throws`). Most developers wrap everything in `RuntimeException` or use unchecked exceptions. Error handling becomes invisible.

**Rust:** `Result<T, E>` is an enum — either `Ok(value)` or `Err(error)`. The `?` operator is the magic: it returns the value on success or propagates the error up the call stack.

```rust
// From config.rs — each env var can fail, ? propagates the error
pub fn from_env() -> Result<Self> {
    Ok(Self {
        telegram_token: std::env::var("TELEGRAM_TOKEN")
            .map_err(|_| BotError::Config("TELEGRAM_TOKEN not set".to_string()))?,  // ? = if Err, return it
        lm_studio_url: std::env::var("LM_STUDIO_URL")
            .map_err(|_| BotError::Config("LM_STUDIO_URL not set".to_string()))?,
        // ...
    })
}
```

**Java equivalent:**
```java
public static Config fromEnv() throws ConfigException {
    return new Config(
        requireEnv("TELEGRAM_TOKEN"),  // throws ConfigException if missing
        requireEnv("LM_STUDIO_URL"),
        // ...
    );
}
```

**The `?` operator unwraps this pattern:**
```rust
// What ? does behind the scenes:
let value = match some_operation() {
    Ok(v) => v,                    // Success: unwrap and continue
    Err(e) => return Err(e.into()), // Error: convert and return early
};

// With ?, it becomes:
let value = some_operation()?;
```

**Critical insight:** `?` calls `.into()` on the error, which means you can use `?` across different error types as long as conversions exist (via `From` trait). This is what makes `#[from]` in `thiserror` so powerful — see next section.

---

### 8. Custom Error Types with `thiserror`

**Java:** You extend `Exception` and create a hierarchy. Error handling is verbose and often swallowed.

**Rust:** The `thiserror` crate generates `Error` trait implementations from an enum. Each variant is a different error case.

```rust
// From error.rs — all possible errors in one enum
use thiserror::Error;

#[derive(Error, Debug)]
pub enum BotError {
    #[error("Telegram error: {0}")]
    Telegram(String),

    #[error("LM Studio error: {0}")]
    LMStudio(String),

    #[error("Home Assistant error: {0}")]
    HomeAssistant(String),

    #[error("Whisper error: {0}")]
    Whisper(String),

    #[error("Database error: {0}")]
    Database(#[from] sqlx::Error),      // #[from] auto-generates From<sqlx::Error>

    #[error("HTTP error: {0}")]
    Http(#[from] reqwest::Error),       // #[from] auto-generates From<reqwest::Error>

    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),

    #[error("Configuration error: {0}")]
    Config(String),
}

// Type alias so we don't write BotError everywhere
pub type Result<T> = std::result::Result<T, BotError>;
```

**Java equivalent:**
```java
public abstract class BotException extends Exception { }
public class TelegramException extends BotException { }
public class LMStudioException extends BotException { }
public class DatabaseException extends BotException {
    public DatabaseException(SQLException cause) { super(cause); }  // = #[from]
}
```

**Why `#[from]` matters:** It generates `impl From<sqlx::Error> for BotError`, which means the `?` operator automatically converts `sqlx::Error` into `BotError::Database`. You write `pool.fetch_one(&query).await?` and the error conversion happens implicitly. No manual `try/catch` wrapping.

---

### 9. The `match` Expression — Exhaustive Pattern Matching

**Java:** `switch` expressions (Java 17+) are similar but not exhaustive for strings.

**Rust:** `match` is exhaustive — the compiler ensures you handle every possible case. For enums, forgetting a variant is a compile error.

```rust
// From plugins/home_assistant.rs — dispatching function calls
async fn execute(&self, function_name: &str, params: Value) -> Result<Value> {
    match function_name {
        "turn_on_light"    => { /* handle */ }
        "turn_off_light"   => { /* handle */ }
        "get_entity_state" => { /* handle */ }
        "set_thermostat"   => { /* handle */ }
        "list_entities"    => self.list_entities().await,
        _ => Err(BotError::HomeAssistant(format!("Unknown function: {}", function_name))),
        // ^ The wildcard `_` catches everything else. Without it: compile error.
    }
}
```

**`match` with `Option` and `Result` (where it really shines):**

```rust
// From telegram.rs — handling the orchestrator result
match response {
    Ok(reply) => {
        bot.send_message(chat_id, reply).await?;
    }
    Err(e) => {
        tracing::error!("Orchestrator error: {}", e);
        bot.send_message(chat_id, "Sorry, something went wrong.").await?;
    }
}
```

**`if let` — match for one specific case:**

```rust
// From state.rs — only do something if pool exists
if let Some(pool) = &self.pool {
    // ... use pool ...
}
// Equivalent to: match &self.pool { Some(pool) => { ... }, None => {} }
```

**`let ... else` — match or early return:**

```rust
// From orchestrator.rs — if no tool calls, return empty vec
let Some(calls) = tool_calls else {
    return Ok(vec![]);
};
// `calls` is now available as the unwrapped value
```

---

### 10. `Box<dyn Trait>` — Dynamic Dispatch (Interface References)

**Java:** `Plugin plugin = new HomeAssistantPlugin();` — you always work through interface references, and the JVM handles dynamic dispatch via vtables.

**Rust:** You must opt into dynamic dispatch explicitly with `dyn`:

```rust
// From plugins/mod.rs — PluginRegistry holds a Vec of trait objects
pub struct PluginRegistry {
    plugins: Vec<Box<dyn Plugin>>,
    //            ^^^            = heap allocated (like Java's new)
    //                ^^^        = dynamic dispatch (like Java's interface reference)
    //                    ^^^^^^ = the trait (like Java's interface type)
}

// From main.rs — registering a concrete type as a trait object
let mut registry = PluginRegistry::new();
registry.register(Box::new(HomeAssistantPlugin::new(
    config.home_assistant_url.clone(),
    config.home_assistant_token.clone(),
)));
```

**Breaking down `Box<dyn Plugin>`:**

| Part | Purpose | Java Equivalent |
|------|---------|-----------------|
| `Box<...>` | Heap allocation (pointer + data) | `new` keyword |
| `dyn Plugin` | Runtime dispatch via vtable | Interface type in variable declaration |
| Combined | "A heap-allocated object implementing Plugin" | `Plugin p = new HomeAssistantPlugin()` |

**Why Java doesn't need `Box`:** In Java, all objects are already heap-allocated and accessed through references. In Rust, values live on the stack by default. `Box` explicitly puts something on the heap — necessary for trait objects because different implementations have different sizes, and the stack needs to know the size at compile time.

**Static vs Dynamic dispatch:** Rust also supports static dispatch with generics (`fn foo<T: Plugin>(p: T)`), where the compiler monomorphizes — generates a separate copy of the function for each concrete type. Zero overhead. Use generics when you know the type at compile time, `dyn` when you don't (like a plugin registry).

---

### 11. `Arc<T>` — Thread-Safe Shared Ownership

**Java:** All object references are shared by default. The GC tracks them. You pass objects to multiple threads freely.

**Rust:** Single ownership is the default. When multiple parts of the program need to own the same data (especially across async tasks), use `Arc` (Atomic Reference Count):

```rust
// From main.rs — sharing the orchestrator across all Telegram message handlers
let orchestrator = Arc::new(Orchestrator::new(lm_provider, registry, model));
let whisper = Arc::new(WhisperProvider::new(config.whisper_url.clone()));

// From telegram.rs — cloning is cheap (just increments the atomic counter)
let orch = Arc::clone(&orchestrator);  // ref count: 1 → 2
let wh = Arc::clone(&whisper);         // ref count: 1 → 2
```

**Java equivalent:**
```java
// In Java, this is invisible — every object reference is essentially an Arc
var orchestrator = new Orchestrator(lmProvider, registry, model);
// Passing `orchestrator` to multiple threads Just Works™ because of GC.
// In Rust, you explicitly choose shared ownership.
```

**How it works:**
- `Arc::new(value)` wraps a value in an atomic reference counter (starts at 1)
- `Arc::clone(&arc)` increments the counter (O(1), very cheap — NOT a deep copy)
- When a clone is dropped, the counter decrements
- When the counter hits 0, the data is freed
- The "Atomic" part means the counter is thread-safe (uses CPU atomic operations)

**`Arc` vs `Rc`:** `Rc` is the single-threaded version (slightly faster but can't cross thread boundaries). The compiler enforces this — try to send an `Rc` to another thread and it won't compile.

---

### 12. `Mutex<T>` — Interior Mutability with Locking

**Java:** `synchronized` blocks or `ReentrantLock` for mutual exclusion. Nothing prevents you from forgetting to lock.

**Rust:** `Mutex<T>` wraps the data itself. You literally cannot access the data without acquiring the lock. The compiler enforces this.

```rust
// You can't do this:
let state: ConversationState = /* ... */;
state.add_message(...);  // If state is shared, this would be a data race

// You must do this:
let state: Mutex<ConversationState> = Mutex::new(/* ... */);
let mut guard = state.lock().await;  // Acquire lock, get MutexGuard
guard.add_message(...);              // Access through the guard
// guard is dropped here → lock released
```

**Two types of Mutex in Rust:**

| Type | When to Use | Java Equivalent |
|------|-------------|-----------------|
| `std::sync::Mutex` | Synchronous code, never held across `.await` | `synchronized` / `ReentrantLock` |
| `tokio::sync::Mutex` | Async code, may be held across `.await` | No direct equivalent |

**This project uses `tokio::sync::Mutex`** because we hold the lock while calling `orch.process_message()`, which is an async function:

```rust
// From telegram.rs — why tokio::sync::Mutex
let response = {
    let mut guard = state.lock().await;         // tokio::sync::Mutex — async-aware
    orch.process_message(text, &mut guard).await // .await while holding lock
};  // guard drops here → lock released BEFORE the HTTP send below

bot.send_message(chat_id, reply).await?;  // No lock held during network I/O
```

If we used `std::sync::Mutex` here, the compiler would reject it because `MutexGuard` from `std::sync` is not `Send` across `.await` points.

---

### 13. `Arc<Mutex<T>>` — Shared Mutable State Across Tasks

The combination of `Arc` + `Mutex` is the standard pattern for sharing mutable state across async tasks. This is the Rust equivalent of Java's shared mutable object with synchronized access.

```rust
// From main.rs — constructing shared state
let shared_state = Arc::new(Mutex::new(state));
//                 ^^^                         = shared ownership (multiple tasks)
//                          ^^^^^              = mutual exclusion (one writer at a time)
//                                ^^^^^        = the actual data

// From telegram.rs — each message handler gets a clone
teloxide::repl(bot, move |bot: Bot, msg: Message| {
    let st = Arc::clone(&st);       // Clone Arc (cheap: ref count++)
    async move {
        let mut guard = st.lock().await;  // Lock to get mutable access
        // ... modify state ...
    }  // guard drops → unlock. Arc clone drops → ref count--.
});
```

**Java equivalent:**
```java
// Java: shared object with synchronized access
private final ConversationState state = new ConversationState();
private final ReentrantLock lock = new ReentrantLock();

public void handleMessage(String text) {
    lock.lock();
    try {
        state.addMessage("user", text);
    } finally {
        lock.unlock();
    }
}
```

**The Rust advantage:** In Java, nothing stops you from accessing `state` without the lock — it's a runtime bug waiting to happen. In Rust, the `Mutex` wraps the data. There is no way to get a reference to `ConversationState` without calling `.lock()`. Thread safety is enforced by the type system.

---

### 14. `async`/`await` and Tokio

**Java:** `CompletableFuture`, virtual threads (Java 21), or reactive libraries like Reactor/RxJava.

**Rust:** `async fn` returns a `Future`. `.await` suspends until the Future completes. `tokio` is the async runtime (like Netty under the hood).

```rust
// From main.rs — #[tokio::main] sets up the async runtime
#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let config = Config::from_env()?;
    let pool = init_db(&config.database_url).await?;   // async DB connection
    // ...
    home_llm_bot::telegram::start(/* ... */).await?;    // blocks until bot shuts down
    Ok(())
}
```

**Java equivalent:**
```java
public static void main(String[] args) throws Exception {
    // Java 21 virtual threads
    var config = Config.fromEnv();
    var pool = initDb(config.databaseUrl).get();  // .get() = .await
    TelegramBot.start(/* ... */).get();
}
```

**Key differences from Java:**
1. **Zero-cost:** Rust futures are state machines compiled by the compiler. No heap allocation per task (unlike `CompletableFuture`).
2. **Lazy:** A Rust future does nothing until `.await`ed. Creating it is free. Java's `CompletableFuture.supplyAsync()` starts immediately.
3. **Cancellation:** Dropping a future cancels it. No `future.cancel(true)` needed.
4. **`Send` bounds:** The compiler ensures that values held across `.await` points are safe to move between threads. This is why `tokio::sync::Mutex` exists (its guard is `Send`; `std::sync::Mutex`'s guard is not).

```rust
// From whisper.rs — async HTTP call
pub async fn transcribe(&self, audio_data: Vec<u8>) -> Result<String> {
    let response = self.client
        .post(format!("{}/v1/audio/transcriptions", self.url))
        .multipart(form)
        .send()        // Returns a Future
        .await?;       // Suspends here until HTTP response arrives

    // Execution resumes here when the response is ready
    let json: Value = response.json().await?;
    // ...
}
```

---

### 15. `move` Closures — Capturing Ownership

**Java:** Lambdas capture effectively-final local variables by reference. You can't modify captured variables.

**Rust:** Closures capture by reference by default. The `move` keyword transfers ownership of captured variables into the closure.

```rust
// From telegram.rs — the teloxide message handler
let orch = Arc::clone(&orchestrator);
let wh = Arc::clone(&whisper);
let st = Arc::clone(&state);

teloxide::repl(bot, move |bot: Bot, msg: Message| {
    //                ^^^^
    // `move` transfers ownership of `orch`, `wh`, `st` INTO the closure.
    // The closure now owns these Arc clones. Without `move`, the closure
    // would try to borrow them — but they'd be dropped at end of `start()`,
    // while the closure lives longer (in the running bot). Compiler rejects this.

    let orch = Arc::clone(&orch);  // Clone AGAIN — each message gets its own Arc clone
    let wh = Arc::clone(&wh);
    let st = Arc::clone(&st);

    async move {
        //    ^^^^
        // Another `move` — this async block takes ownership of the per-message clones.
        // Each spawned task has its own set of Arc clones.
        handle_message(bot, msg, orch, wh, st).await
    }
});
```

**Java equivalent:**
```java
// Java captures by reference (effectively final)
var orch = orchestrator;  // effectively final
bot.onMessage(msg -> {
    // `orch` is captured by reference. Java's GC keeps it alive.
    orch.processMessage(msg);
});
// In Rust, there's no GC. `move` tells the compiler: "the closure now owns these values"
```

**Why two levels of `move`?**
1. The outer `move` closure captures `orch`, `wh`, `st` — these live for the lifetime of the bot
2. Inside, we clone them per-message, and the `async move` block takes ownership of those clones
3. Each concurrent message handler has its own independent `Arc` clone — no lifetime issues

---

### 16. Iterator Chains and `.collect()`

**Java:** Streams (`stream().map().filter().collect()`).

**Rust:** Iterators are very similar but zero-cost — the compiler fuses the chain into a single loop.

```rust
// From orchestrator.rs — building the OpenAI messages array
let messages: Vec<Value> = context
    .iter()                                           // Iterator<&Message>
    .map(|m| json!({"role": m.role, "content": m.content}))  // Iterator<Value>
    .collect();                                       // Vec<Value>

// From orchestrator.rs — building tools array from all plugins
let tools: Vec<Value> = self.registry
    .get_all_functions()           // Vec<FunctionDef>
    .into_iter()                   // consumes the Vec, moves elements
    .map(|f| f.to_openai_tool())   // FunctionDef → Value
    .collect();                    // Vec<Value>
```

**Java equivalent:**
```java
var messages = context.stream()
    .map(m -> Map.of("role", m.role, "content", m.content))
    .collect(Collectors.toList());
```

**Collecting into `Result` — the Rust-specific superpower:**

```rust
// From orchestrator.rs — parsing function calls, short-circuiting on first error
calls
    .iter()
    .map(|tc| {
        let name = tc["function"]["name"].as_str()
            .ok_or_else(|| BotError::LMStudio("Missing name".into()))?;
        let parameters: Value = serde_json::from_str(args_str)?;
        Ok(FunctionCall { name: name.to_string(), parameters })
    })
    .collect()  // Collects Iterator<Result<FunctionCall>> into Result<Vec<FunctionCall>>
                // If ANY item is Err, the whole collect returns Err. First error wins.
```

This pattern — `.map(|x| { ...; Ok(transformed) }).collect::<Result<Vec<_>>>()` — is idiomatic Rust for "transform a collection where each transformation can fail." Java has no equivalent; you'd need a for-loop with try/catch.

**`.iter()` vs `.into_iter()` vs `.iter_mut()`:**

| Method | Yields | Ownership | Java Equivalent |
|--------|--------|-----------|-----------------|
| `.iter()` | `&T` | Borrows, collection unchanged | `.stream()` |
| `.iter_mut()` | `&mut T` | Mutable borrows | — (no Java equivalent) |
| `.into_iter()` | `T` | Consumes collection | `.stream()` (Java doesn't distinguish) |

---

### 17. The Module System and Crate Structure

**Java:** Packages, jar files, classpath. One public class per file (by convention).

**Rust:** Modules, crates, Cargo. Module tree is declared explicitly.

```rust
// src/lib.rs — declares all modules (like a package-info.java for the whole library)
pub mod error;
pub mod config;
pub mod state;
pub mod plugins;
pub mod telegram;
pub mod orchestrator;

// src/plugins/mod.rs — declares sub-modules (like a sub-package)
pub mod home_assistant;
pub mod lm_studio;
pub mod whisper;
```

**This project is both a library and a binary:**

```toml
# Cargo.toml
[lib]
name = "home_llm_bot"
path = "src/lib.rs"      # Library crate — can be tested, reused

[[bin]]
name = "home-llm-bot"
path = "src/main.rs"     # Binary crate — the executable
```

**Java equivalent:** Like having both a JAR library and a main class. Tests run against the library. The binary imports from the library:

```rust
// src/main.rs — imports from the library crate
use home_llm_bot::{
    config::Config,
    orchestrator::Orchestrator,
    plugins::{PluginRegistry, home_assistant::HomeAssistantPlugin, /* ... */},
    state::{ConversationState, init_db},
};
```

**Visibility:**

| Rust | Java | Scope |
|------|------|-------|
| (default) | package-private | Same module only |
| `pub` | `public` | Everywhere |
| `pub(crate)` | — (no equivalent) | Same crate only |
| `pub(super)` | `protected` (roughly) | Parent module |

---

### 18. Derive Macros — Auto-Implementing Traits

**Java:** `@Data` (Lombok), `record` (Java 16) auto-generate boilerplate.

**Rust:** `#[derive(...)]` auto-implements traits:

```rust
// From state.rs
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Message {
    pub role: String,
    pub content: String,
    pub timestamp: DateTime<Utc>,
    pub sender_name: Option<String>,
}
```

| Derive | Java Equivalent | What It Generates |
|--------|-----------------|-------------------|
| `Clone` | `.clone()` method | Deep copy |
| `Debug` | `toString()` (debug format) | `{:?}` formatting |
| `Serialize` | Jackson `@JsonProperty` | JSON/other format serialization |
| `Deserialize` | Jackson `@JsonCreator` | JSON/other format deserialization |
| `PartialEq` | `equals()` | Equality comparison |
| `Hash` | `hashCode()` | Hash computation |
| `Default` | No-arg constructor | Default values for all fields |

```rust
// From error.rs — thiserror's Error derive
#[derive(Error, Debug)]
pub enum BotError {
    #[error("Database error: {0}")]
    Database(#[from] sqlx::Error),
    // The derive macro generates:
    // - impl std::fmt::Display for BotError (the #[error("...")] messages)
    // - impl std::error::Error for BotError
    // - impl From<sqlx::Error> for BotError (the #[from] attribute)
}
```

**Java equivalent with Lombok:**
```java
@Data @AllArgsConstructor
public class Message {
    private String role;
    private String content;
    private Instant timestamp;
    @Nullable private String senderName;
}
```

---

### 19. The Builder Pattern (Consuming `self`)

**Java:** Builder pattern returns `this` for method chaining. You can call methods in any order and call `.build()` when done.

**Rust:** Many builders consume `self` (not `&mut self`), which means each method returns a new value. This prevents using a builder after it's been finalized.

```rust
// From whisper.rs — reqwest's multipart builder
let form = multipart::Form::new()     // Creates Form (owned)
    .part("file", audio_part)          // Consumes Form, returns new Form with part added
    .text("model", "whisper-1");       // Consumes Form, returns new Form with text added
// `form` is the final value. The intermediate Forms are consumed and gone.

// From telegram.rs — teloxide message builder
bot.send_message(chat_id, format!("Heard: *{}*", transcription))
    .parse_mode(teloxide::types::ParseMode::MarkdownV2)  // Consumes, returns enhanced request
    .await?;                                               // Sends it
```

**Java equivalent:**
```java
var form = new MultipartForm()
    .addPart("file", audioPart)     // Returns `this`
    .addText("model", "whisper-1"); // Returns `this`
// In Java, `form` and the intermediate values are the same object.
// In Rust, ownership transfer prevents accidental reuse after .await
```

**Why consume `self`?** It encodes state transitions in the type system. Once you call `.send()` on a request builder, the builder is consumed — you can't accidentally send it twice. In Java, nothing prevents calling `.execute()` twice on the same request builder.

---

### 20. Testing

**Java:** JUnit, `@Test`, `@BeforeEach`, separate test source directory (`src/test/java`).

**Rust:** Tests live in the same file as the code they test, inside a `#[cfg(test)] mod tests` block. This is one of Rust's best features — tests are always next to the code.

```rust
// From state.rs — tests at the bottom of the same file
#[cfg(test)]                    // Only compiled when running `cargo test`
mod tests {
    use super::*;               // Import everything from the parent module

    #[test]                     // Synchronous test (like JUnit @Test)
    fn test_add_message_to_history() {
        let mut state = ConversationState::new(1);
        state.add_message("user", "Turn on lights", None);
        assert_eq!(state.messages.len(), 1);
        assert_eq!(state.messages[0].role, "user");
    }

    #[tokio::test]              // Async test (needs tokio runtime)
    async fn test_persist_and_load_message() {
        let pool = init_db("sqlite::memory:").await.unwrap();
        let mut state = ConversationState::with_db(1, pool.clone());
        state.add_message_persisted("user", "hello", None).await.unwrap();
        let history = ConversationState::load_history(&pool, 1, 10).await.unwrap();
        assert_eq!(history.len(), 1);
    }

    #[tokio::test]
    #[ignore]                   // Skipped by default — run with `cargo test -- --ignored`
    async fn test_real_turn_on_light() {
        // Integration test requiring real Home Assistant
    }
}
```

**Java equivalent:**
```java
// In Java, this would be in a SEPARATE file: src/test/java/StateTest.java
class StateTest {
    @Test
    void testAddMessageToHistory() {
        var state = new ConversationState(1);
        state.addMessage("user", "Turn on lights", null);
        assertEquals(1, state.getMessages().size());
    }
}
```

**Key differences:**

| Feature | Rust | Java |
|---------|------|------|
| Test location | Same file (`#[cfg(test)]`) | Separate `test/` directory |
| Test discovery | `#[test]` attribute | `@Test` annotation |
| Async tests | `#[tokio::test]` | TestNG `@Test` or JUnit with extensions |
| Skip tests | `#[ignore]` | `@Disabled` |
| Setup/teardown | None built-in (use constructors) | `@BeforeEach` / `@AfterEach` |
| Assertions | `assert_eq!`, `assert!` (macros) | `assertEquals()`, `assertTrue()` |
| Run all | `cargo test` | `mvn test` / `gradle test` |

**Why tests in the same file?** It keeps tests close to the code they test. When you modify a function, the tests are right there — no jumping between directories. The `#[cfg(test)]` means test code is completely excluded from the release binary. Zero cost.

---

### Quick Reference Card

| Rust | Java | Notes |
|------|------|-------|
| `let x = 5;` | `final var x = 5;` | Immutable by default |
| `let mut x = 5;` | `var x = 5;` | Explicit mutability |
| `fn foo(x: i32) -> i32` | `int foo(int x)` | Return type after `->` |
| `String` | `String` | Owned, heap-allocated |
| `&str` | `String` (parameter) | Borrowed string slice |
| `Vec<T>` | `ArrayList<T>` | Growable array |
| `HashMap<K, V>` | `HashMap<K, V>` | Same concept |
| `Option<T>` | `@Nullable T` | Compile-time null safety |
| `Result<T, E>` | `throws Exception` | Explicit error handling |
| `?` operator | `throws` (propagation) | Early return on error |
| `Box<T>` | `new T()` | Heap allocation |
| `Arc<T>` | GC reference | Thread-safe shared ownership |
| `Mutex<T>` | `synchronized` | Compiler-enforced locking |
| `trait Foo` | `interface Foo` | No inheritance |
| `impl Foo for Bar` | `class Bar implements Foo` | Trait implementation |
| `dyn Foo` | `Foo foo =` | Dynamic dispatch |
| `async fn` / `.await` | `CompletableFuture` | Zero-cost async |
| `#[derive(...)]` | `@Data` (Lombok) | Auto-implement traits |
| `match` | `switch` expression | Exhaustive |
| `\|x\| x + 1` | `x -> x + 1` | Closure / lambda |
| `move \|\|` | Lambda capturing | Explicit ownership transfer |
| `.iter().map().collect()` | `.stream().map().collect()` | Zero-cost iterators |
| `cargo build` | `mvn package` | Build |
| `cargo test` | `mvn test` | Test |
| `cargo run` | `mvn exec:java` | Run |
