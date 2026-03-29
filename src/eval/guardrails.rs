use anyhow::{Result, bail};

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Threshold {
    Absolute(f64),
    RelativePercent(f64),
}

pub fn parse_threshold(spec: &str) -> Result<Threshold> {
    let trimmed = spec.trim();
    if let Some(percent) = trimmed.strip_suffix('%') {
        return Ok(Threshold::RelativePercent(percent.parse::<f64>()?));
    }
    if trimmed.is_empty() {
        bail!("threshold cannot be empty");
    }
    Ok(Threshold::Absolute(trimmed.parse::<f64>()?))
}

pub fn passes_threshold(current: f64, baseline: f64, threshold: Threshold) -> Result<bool> {
    match threshold {
        Threshold::Absolute(limit) => Ok((current - baseline) <= limit),
        Threshold::RelativePercent(limit) => {
            if baseline == 0.0 {
                bail!("cannot compute relative threshold from a zero baseline");
            }
            let delta_percent = ((current - baseline) / baseline) * 100.0;
            Ok(delta_percent <= limit)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{Threshold, parse_threshold, passes_threshold};

    #[test]
    fn parses_relative_thresholds() {
        assert_eq!(
            parse_threshold("+10%").expect("threshold should parse"),
            Threshold::RelativePercent(10.0)
        );
    }

    #[test]
    fn checks_absolute_thresholds() {
        let passed = passes_threshold(105.0, 100.0, Threshold::Absolute(10.0))
            .expect("threshold check should work");
        assert!(passed);
    }
}
