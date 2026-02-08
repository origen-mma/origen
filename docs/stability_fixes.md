# Stability Fixes for t0 Recovery

## Problem
After implementing the normalization fix to eliminate systematic bias, the optimizer became unstable:
- 25% failure rate with catastrophic ELBO values (some as low as -113 million!)
- High variance in results across runs
- Multi-start optimization didn't help (all starts could fail)

## Root Causes Identified

1. **Extreme scale factors**: When max_pred is very small or large, `scale = 1.0 / max_pred` can be extreme (0.001 or 1000), causing numerical instability

2. **High learning rate**: After renormalization changes gradient magnitudes, the original LR=0.01 may be too aggressive

3. **Cascading numerical errors**: NaN/Inf values not caught early enough, propagating through optimization

## Fixes Implemented

### Fix 1: Numerical Safeguards in Renormalization

**Files**:
- [pso_fitter.rs:70-103](/Users/mcoughlin/Code/ORIGIN/origin/crates/mm-core/src/pso_fitter.rs#L70-L103)
- [svi_fitter.rs:214-254](/Users/mcoughlin/Code/ORIGIN/origin/crates/mm-core/src/svi_fitter.rs#L214-L254)

**Changes**:
```rust
// Clamp scale factor to [0.1, 10.0]
let scale_clamped = scale.clamp(0.1, 10.0);

// PSO: Return high cost if scale is extreme
if (scale - scale_clamped).abs() > 0.01 {
    return Ok(1e10);  // Guide PSO away from bad regions
}

// SVI: Skip Monte Carlo sample if scale is extreme
if (scale - scale_clamped).abs() / scale > 0.5 {
    continue;  // Skip this sample
}

// Safety check after scaling
if !pred.is_finite() {
    return Ok(1e10);  // PSO
    continue;         // SVI
}
```

**Rationale**:
- Scale factors outside [0.1, 10.0] indicate the model is in a bad parameter region
- Instead of proceeding with extreme values, guide optimizer away
- Prevents cascading numerical errors

### Fix 2: Lower Learning Rate

**File**: [lightcurve_fitting.rs:349](/Users/mcoughlin/Code/ORIGIN/origin/crates/mm-core/src/lightcurve_fitting.rs#L349)

**Change**:
```rust
// Before
let learning_rate = 0.01;

// After
let learning_rate = 0.005;  // Lowered for stability after normalization fix
```

**Rationale**:
- Renormalization changes gradient magnitudes
- Lower LR = more stable optimization at cost of slightly slower convergence
- 5000 iterations is enough to converge with LR=0.005

### Fix 3: Early Detection and Skipping

**Implementation**:
- PSO returns high cost (1e10) for bad parameter regions
- SVI skips Monte Carlo samples with extreme scale factors
- Both check `pred.is_finite()` after scaling

**Rationale**:
- Better to skip bad samples than let them corrupt gradient estimates
- Prevents catastrophic ELBO values from numerical overflow

## Expected Results

### Before Fixes
```
Failure rate: 25-30%
Catastrophic ELBO: Yes (down to -113M)
Multi-start effective: No (all starts could fail)
```

### After Fixes (Expected)
```
Failure rate: <10% (target)
Catastrophic ELBO: Eliminated (ELBO > -1000)
Multi-start effective: Yes (at least one good start)
Median t0 error: ~0.5 days (for good fits)
```

## Testing

### Quick Test
```bash
cargo test --test debug_catastrophic_elbo -- --nocapture --ignored
```

Runs 10 trials and reports:
- Success rate by quality level
- Presence of catastrophic failures
- Median t0 error for good fits

### Multi-Start Test
```bash
cargo test --test test_multistart test_multistart_statistics -- --nocapture --ignored
```

Tests if multi-start can rescue bad cases.

### Variance Test
```bash
cargo test --test verify_bias_fix -- --nocapture --ignored
```

Measures scatter and bias in t0 recovery.

## Trade-offs

| Aspect | Before | After | Impact |
|--------|--------|-------|--------|
| Learning rate | 0.01 | 0.005 | 2x slower per iteration, but more stable |
| Runtime | ~25s | ~25s | No change (same # iterations) |
| Scale factor range | Unbounded | [0.1, 10.0] | May miss extreme solutions (acceptable) |
| Bad parameter handling | Continue | Skip/penalize | Cleaner optimization landscape |

## Future Improvements

1. **Adaptive learning rate**: Start high (0.01), decay to low (0.001) for fine-tuning
2. **Gradient clipping**: Clip gradient norms to prevent explosion
3. **Better initialization**: Warm-start SVI from better PSO solution
4. **Ensemble**: Average predictions from multiple good fits
5. **Bayesian model averaging**: Weight predictions by ELBO

## References

- Original bias fix: [t0_bias_fix.md](/Users/mcoughlin/Code/ORIGIN/origin/docs/t0_bias_fix.md)
- Diagnostic results: [diagnose_outliers test](/Users/mcoughlin/Code/ORIGIN/origin/crates/mm-core/tests/diagnose_outliers.rs)
- Quality assessment: [fit_quality.rs](/Users/mcoughlin/Code/ORIGIN/origin/crates/mm-core/src/fit_quality.rs)
