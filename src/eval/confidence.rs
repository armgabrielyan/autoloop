pub fn confidence_score(
    improvement: f64,
    observations: &[f64],
    min_experiments: usize,
) -> Option<f64> {
    if observations.len() < min_experiments {
        return None;
    }

    let mad = median_absolute_deviation(observations)?;
    if mad == 0.0 {
        return None;
    }

    Some(improvement.abs() / mad)
}

pub fn median_absolute_deviation(values: &[f64]) -> Option<f64> {
    if values.is_empty() {
        return None;
    }

    let center = median(values)?;
    let deviations: Vec<f64> = values.iter().map(|value| (value - center).abs()).collect();
    median(&deviations)
}

fn median(values: &[f64]) -> Option<f64> {
    if values.is_empty() {
        return None;
    }

    let mut sorted = values.to_vec();
    sorted.sort_by(|left, right| left.total_cmp(right));
    let midpoint = sorted.len() / 2;

    if sorted.len() % 2 == 0 {
        Some((sorted[midpoint - 1] + sorted[midpoint]) / 2.0)
    } else {
        Some(sorted[midpoint])
    }
}

#[cfg(test)]
mod tests {
    use super::{confidence_score, median_absolute_deviation};

    #[test]
    fn computes_mad() {
        let values = [10.0, 12.0, 12.0, 14.0, 100.0];
        let mad = median_absolute_deviation(&values).expect("mad should be present");
        assert_eq!(mad, 2.0);
    }

    #[test]
    fn computes_confidence() {
        let values = [45.0, 44.5, 45.5, 44.0, 46.0];
        let confidence = confidence_score(-1.5, &values, 3).expect("confidence should exist");
        assert!(confidence > 1.0);
    }
}
