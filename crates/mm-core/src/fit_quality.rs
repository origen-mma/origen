//! Fit quality assessment and filtering
//!
//! Provides metrics and filters to identify poor-quality fits
//! that may have large parameter errors due to optimizer failures.

use crate::lightcurve_fitting::LightCurveFitResult;

/// Fit quality classification
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FitQuality {
    /// Excellent fit (ELBO > 50)
    Excellent,
    /// Good fit (ELBO 10-50)
    Good,
    /// Fair fit (ELBO 0-10)
    Fair,
    /// Poor fit (ELBO -10 to 0)
    Poor,
    /// Failed fit (ELBO < -10) - likely optimizer failure
    Failed,
}

impl FitQuality {
    /// Classify fit quality based on ELBO
    pub fn from_elbo(elbo: f64) -> Self {
        if elbo > 50.0 {
            FitQuality::Excellent
        } else if elbo > 10.0 {
            FitQuality::Good
        } else if elbo > 0.0 {
            FitQuality::Fair
        } else if elbo > -10.0 {
            FitQuality::Poor
        } else {
            FitQuality::Failed
        }
    }

    /// Should this fit be accepted for scientific use?
    pub fn is_acceptable(&self) -> bool {
        matches!(
            self,
            FitQuality::Excellent | FitQuality::Good | FitQuality::Fair
        )
    }

    /// Should this fit be rejected outright?
    pub fn is_failed(&self) -> bool {
        matches!(self, FitQuality::Failed)
    }

    /// Human-readable description
    pub fn description(&self) -> &'static str {
        match self {
            FitQuality::Excellent => "Excellent fit - high confidence in parameters",
            FitQuality::Good => "Good fit - reliable parameters",
            FitQuality::Fair => "Fair fit - parameters likely reliable",
            FitQuality::Poor => "Poor fit - review parameters carefully",
            FitQuality::Failed => "Failed fit - optimizer likely stuck, parameters unreliable",
        }
    }
}

/// Comprehensive fit quality assessment
pub struct FitQualityAssessment {
    pub quality: FitQuality,
    pub elbo: f64,
    pub warnings: Vec<String>,
    pub is_acceptable: bool,
}

impl FitQualityAssessment {
    /// Assess fit quality with multiple checks
    pub fn assess(fit: &LightCurveFitResult, first_detection_time: Option<f64>) -> Self {
        let quality = FitQuality::from_elbo(fit.elbo);
        let mut warnings = Vec::new();

        // Check ELBO
        if fit.elbo < -10.0 {
            warnings.push(format!(
                "Very negative ELBO ({:.1}): Optimizer likely stuck in bad local minimum",
                fit.elbo
            ));
        } else if fit.elbo < 0.0 {
            warnings.push(format!(
                "Negative ELBO ({:.1}): Fit quality may be poor",
                fit.elbo
            ));
        }

        // Check t0 uncertainty
        if fit.t0_err < 0.01 {
            warnings.push(format!(
                "Unusually small t0 uncertainty ({:.4} days): Optimizer may be stuck at boundary",
                fit.t0_err
            ));
        }

        // Check t0 plausibility if first detection time is known
        if let Some(first_det) = first_detection_time {
            let t0_offset = (fit.t0 - first_det).abs();
            if t0_offset > 10.0 {
                warnings.push(format!(
                    "t0 is {:.1} days from first detection: Physically implausible for kilonovae",
                    t0_offset
                ));
            }
        }

        // Check if converged
        if !fit.converged {
            warnings.push("Optimizer did not converge within iteration limit".to_string());
        }

        let is_acceptable = quality.is_acceptable() && warnings.len() < 3;

        Self {
            quality,
            elbo: fit.elbo,
            warnings,
            is_acceptable,
        }
    }

    /// Generate warning message for logging
    pub fn warning_message(&self) -> Option<String> {
        if self.warnings.is_empty() {
            None
        } else {
            Some(format!(
                "Fit quality: {:?} (ELBO={:.2}). Warnings:\n  - {}",
                self.quality,
                self.elbo,
                self.warnings.join("\n  - ")
            ))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_quality_classification() {
        assert_eq!(FitQuality::from_elbo(60.0), FitQuality::Excellent);
        assert_eq!(FitQuality::from_elbo(30.0), FitQuality::Good);
        assert_eq!(FitQuality::from_elbo(5.0), FitQuality::Fair);
        assert_eq!(FitQuality::from_elbo(-5.0), FitQuality::Poor);
        assert_eq!(FitQuality::from_elbo(-20.0), FitQuality::Failed);
    }

    #[test]
    fn test_acceptability() {
        assert!(FitQuality::Excellent.is_acceptable());
        assert!(FitQuality::Good.is_acceptable());
        assert!(FitQuality::Fair.is_acceptable());
        assert!(!FitQuality::Poor.is_acceptable());
        assert!(!FitQuality::Failed.is_acceptable());
    }
}
