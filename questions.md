# Research Questions

The analysis is not limited to any one pair of derives, but instead considers the full spectrum of derive attributes used across Rust projects. With that in mind, the project is guided by the following questions:

1. **Prevalence of ordering conventions:**
   When developers use multiple derives (e.g. `Debug`, `Eq`, `PartialEq`, `Serialize`, `Deserialize`, etc.), are there common orderings that consistently appear across codebases?

2. **Consistency within projects:**
   Do individual projects tend to adopt and stick to a specific ordering of derives, or does ordering fluctuate arbitrarily even within the same repository?

3. **Variation across communities and libraries:**
   Are there differences in derive ordering practices between major domains of Rust code, such as the standard library, popular crates, frameworks, or application code?

4. **Patterns versus noise:**
   Across all derives, does the data reveal stable conventions that reflect community norms, or is ordering largely arbitrary and inconsistent?

5. **Implications for formatting tools:**
   If clear patterns do emerge, can they provide a basis for proposing a standard ordering to be adopted by tools like `cargo fmt`/`rustfmt`, improving consistency automatically?
