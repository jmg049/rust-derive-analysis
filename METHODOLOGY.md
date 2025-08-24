# Rust Derive Ordering Analysis: Methodology

## Overview

This document explains the statistical methods and algorithms used to analyze derive ordering patterns in Rust codebases. The analysis aims to identify consistent patterns that could inform rustfmt's derive ordering decisions.

## Research Questions

The analysis addresses five key questions:

1. **Prevalence of ordering conventions**: What ordering patterns are most common across Rust codebases?
2. **Consistency within projects**: How consistent are individual repositories in their derive ordering?
3. **Variation across communities**: Do different types of projects (web, CLI, gamedev) have different ordering preferences?
4. **Patterns versus noise**: Which observed patterns are statistically significant vs random variation?
5. **Implications for formatting tools**: What concrete recommendations can we make for rustfmt?

## Data Collection

### Repository Selection
- Popular Rust repositories from GitHub (typically 100+ stars)
- Diverse ecosystem representation (web frameworks, CLI tools, game engines, etc.)
- Focus on well-maintained, actively developed projects

### Derive Statement Extraction
- Parse Rust source files using the `syn` crate
- Extract `#[derive(...)]` attributes from struct and enum definitions
- Fall back to text-based parsing for files that cause syn to crash
- Filter out generated code and test files where possible

## Methodology

### 1. Data Preprocessing

#### Derive Parsing
Each derive statement like `#[derive(Debug, Clone, Copy)]` is parsed into:
- **Individual traits**: `["Debug", "Clone", "Copy"]`
- **Adjacent pairs**: `[("Debug", "Clone"), ("Clone", "Copy")]`
- **All combinations**: `[("Debug", "Clone"), ("Debug", "Copy"), ("Clone", "Copy")]`

#### Filtering
- Focus on multi-derive statements (2+ traits) for ordering analysis
- Single-derive statements are counted but don't contribute to ordering patterns

### 2. Consistency Score Calculation

#### What is a Consistency Score?
A consistency score measures how predictable a repository's derive ordering is. It ranges from 0 (completely random) to 1 (perfectly consistent).

#### Mathematical Approach: Entropy-Based Scoring

**Step 1: Count Pattern Frequencies**
For each repository, count how often each derive pair appears:
```
Repository A:
- Debug → Clone: 15 times
- Clone → Debug: 3 times
- Clone → Copy: 8 times
- Copy → Clone: 2 times
```

**Step 2: Calculate Probabilities**
Convert counts to probabilities:
```
Total pairs: 28
- Debug → Clone: 15/28 = 0.536
- Clone → Debug: 3/28 = 0.107
- Clone → Copy: 8/28 = 0.286
- Copy → Clone: 2/28 = 0.071
```

**Step 3: Calculate Entropy**
Entropy measures the unpredictability of the patterns:
```
H = -Σ(p × log₂(p))
H = -(0.536×log₂(0.536) + 0.107×log₂(0.107) + 0.286×log₂(0.286) + 0.071×log₂(0.071))
H ≈ 1.73 bits
```

**Step 4: Normalize by Maximum Entropy**
Maximum entropy occurs when all patterns are equally likely:
```
Max H = log₂(number of unique patterns) = log₂(4) = 2.0 bits
Normalized entropy = 1.73 / 2.0 = 0.865
```

**Step 5: Convert to Consistency Score**
```
Consistency = 1 - Normalized Entropy = 1 - 0.865 = 0.135
```

#### Interpretation
- **0.0-0.3**: Low consistency (chaotic ordering)
- **0.3-0.7**: Moderate consistency (some patterns but variation)
- **0.7-1.0**: High consistency (strong patterns)

### 3. Statistical Significance Testing

#### Binomial Tests for Ordering Preferences
For each derive pair (e.g., Debug vs Clone), we test if the observed ordering preference is statistically significant.

**Null Hypothesis**: No ordering preference (50/50 split)
**Alternative Hypothesis**: Significant ordering preference exists

**Example**:
```
Debug → Clone: 45 occurrences
Clone → Debug: 15 occurrences
Total: 60 occurrences

Binomial test:
- Expected under null hypothesis: 30/30 split
- Observed: 45/15 split
- p-value ≈ 0.0003 (highly significant)
- Effect size: |45-15|/60 = 0.5 (strong preference)
```

#### Bootstrap Confidence Intervals
To ensure our patterns are stable:

1. **Resample**: Create 1000 bootstrap samples of the data
2. **Recalculate**: Measure pattern frequencies in each sample
3. **Confidence intervals**: Calculate 95% confidence bounds
4. **Stability**: Patterns with narrow confidence intervals are more reliable

#### Multiple Testing Correction
When testing many derive pairs simultaneously, we account for multiple comparisons using the Benjamini-Hochberg procedure to control false discovery rate.

### 4. Domain Categorization

#### Repository Classification
Repositories are categorized by domain to identify community-specific patterns:

- **Web frameworks**: actix-web, axum, rocket, warp
- **CLI tools**: ripgrep, fd, bat, starship
- **Game development**: bevy, winit, wgpu
- **Development tools**: rust-analyzer, cargo, clippy
- **Data processing**: polars, datafusion
- **Systems**: redox OS, embedded projects
- **Other**: Miscellaneous projects

#### Statistical Comparison
Use Analysis of Variance (ANOVA) to test for significant differences in consistency scores across domains:

**Null Hypothesis**: All domains have equal mean consistency
**Alternative Hypothesis**: At least one domain differs significantly

### 5. Pattern vs Noise Detection

#### Entropy Analysis
Calculate the overall entropy of derive ordering patterns:

- **Low entropy** (< 0.7): Strong structural patterns exist
- **High entropy** (> 0.9): Mostly random ordering
- **Medium entropy** (0.7-0.9): Some structure with variation

#### Effect Size Calculation
For each significant pattern, calculate Cohen's d or similar effect size measures to distinguish between:
- **Statistically significant**: p < 0.05 (unlikely due to chance)
- **Practically significant**: Large enough effect to matter in practice

## Visualization Techniques

### 1. Frequency Distributions
- **Bar charts**: Most common individual derives and derive pairs
- **Histograms**: Distribution of consistency scores across repositories

### 2. Heatmaps
- **Preference matrices**: Visual representation of derive pair ordering preferences
- **Color coding**: Red = strong preference for row→column ordering, Blue = reverse preference

### 3. Scatter Plots
- **Consistency vs Size**: Relationship between repository size and ordering consistency
- **Statistical significance**: Effect size vs occurrence frequency with significance color-coding

### 4. Box Plots
- **Domain comparison**: Consistency score distributions across repository categories
- **Outlier identification**: Repositories with unusually high/low consistency

## Limitations and Assumptions

### Data Limitations
1. **Sample bias**: Popular GitHub repositories may not represent all Rust code
2. **Temporal effects**: Ordering preferences may evolve over time
3. **Generated code**: Some derives may be auto-generated with different patterns

### Statistical Assumptions
1. **Independence**: Assume derive statements within repositories are independent
2. **Stationarity**: Assume ordering preferences don't change within a repository
3. **Representativeness**: Assume analyzed repositories represent broader Rust ecosystem

### Methodological Limitations
1. **Context ignorance**: Don't consider semantic relationships between derives
2. **Author preference**: Individual developer preferences may dominate small repositories
3. **Tool influence**: Existing formatter usage may bias current patterns

## Validation Approaches

### 1. Cross-Validation
- Split repositories into training/validation sets
- Verify patterns hold across different repository samples

### 2. Temporal Analysis
- Compare patterns in older vs newer code
- Check for evolution of ordering preferences over time

### 3. Manual Review
- Manually inspect high-confidence recommendations
- Verify they align with Rust community conventions and best practices

## Recommendations Generation

### Confidence Levels
**High Confidence** (p < 0.001, effect size > 0.6):
- Strong statistical evidence
- Large practical effect
- Suitable for automatic formatting rules

**Medium Confidence** (p < 0.05, effect size > 0.4):
- Moderate statistical evidence
- Noticeable practical effect
- Consider for optional formatting rules

**Low Confidence** (p < 0.05, effect size > 0.2):
- Weak but significant evidence
- Small practical effect
- Document but don't implement automatically

### Implementation Considerations
1. **Coverage**: What percentage of real-world code would be affected?
2. **Backwards compatibility**: Would changes break existing code formatting?
3. **Edge cases**: How to handle uncommon derive combinations?
4. **User control**: Should users be able to override recommendations?

## Reproducibility

### Code Availability
All analysis code is provided in the accompanying Jupyter notebook with:
- Detailed comments explaining each step
- Modular functions for easy modification
- Visualization code for generating all figures

### Data Availability
- Raw derive statement data exported to CSV
- Intermediate analysis results saved for inspection
- Statistical test results documented with confidence intervals

### Environment Requirements
- Python 3.8+ with pandas, numpy, scipy, matplotlib, seaborn
- Rust toolchain for data collection (cargo, rustc)
- Jupyter notebook environment for interactive analysis

---

This methodology provides a rigorous, statistical foundation for making evidence-based recommendations about derive ordering in Rust code formatting tools.