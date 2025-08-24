# What Order are People Ordering Their Rust Derives?

In the ``Rust`` programming language there is a feature which allows a ``struct`` to *derive* behaviour. In reality, like all of Rust's *annotation-like* syntax, these are macros which at compile time, insert the code so you don't have to.

```rust
#[derive(Clone, Copy)]
pub struct MyStruct;
```

The above codefence trivially demonstrates the derive syntax. So does

```rust
#[derive(Copy, Clone)]
pub struct MyStruct;
```

While not a massive deal, Rust (and ``cargo fmt``) do not really care about the ordering of derive arguments. This sometimes has to be considered as discussed [here](https://internals.rust-lang.org/t/question-does-rust-as-a-language-require-derive-macros-be-kept-in-order/15947), but mostly the ordering is not important.

This irritates me -- not because I care about the exact order, I care about consistency! If ``cargo fmt`` took care of this, then I would never had realised. But now I must know.

So this project aims to address this via several phases:

## Phase 1: Data Acquistion

- Crawl Rust codebases (including stdlib)
- Extract derive statements
- Persist to disk

## Phase 2: Data Processing

- Process the recently persisted data
- Analyse ordering across all codebases
- Analyse ordering within codebases
- Interpret


## Phase 3: Patch Cargo Fmt

- Provided all goes well, use insight from Phase 2 to create a pull request for cargo fmt and rustfmt
