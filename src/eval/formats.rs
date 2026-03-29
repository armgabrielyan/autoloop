use anyhow::{Context, Result, bail};
use regex::Regex;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MetricFormat {
    #[default]
    Auto,
    MetricLines,
    Json,
    Regex,
}

pub fn parse_metric_value(
    format: MetricFormat,
    output: &str,
    metric_name: &str,
    regex: Option<&Regex>,
) -> Result<f64> {
    match format {
        MetricFormat::Auto => parse_metric_lines(output, metric_name)
            .or_else(|_| parse_json_metric(output, metric_name))
            .or_else(|_| parse_regex_metric(output, regex)),
        MetricFormat::MetricLines => parse_metric_lines(output, metric_name),
        MetricFormat::Json => parse_json_metric(output, metric_name),
        MetricFormat::Regex => parse_regex_metric(output, regex),
    }
}

fn parse_metric_lines(output: &str, metric_name: &str) -> Result<f64> {
    let needle = format!("METRIC {metric_name}=");
    for line in output.lines() {
        let trimmed = line.trim();
        if let Some(value) = trimmed.strip_prefix(&needle) {
            return value
                .trim()
                .parse::<f64>()
                .with_context(|| format!("failed to parse metric value from `{trimmed}`"));
        }
    }

    bail!("metric `{metric_name}` not found in METRIC lines")
}

fn parse_json_metric(output: &str, metric_name: &str) -> Result<f64> {
    let parsed: serde_json::Value =
        serde_json::from_str(output).context("failed to parse command output as JSON")?;
    let Some(metrics) = parsed.get("metrics") else {
        bail!("JSON output did not contain a `metrics` object");
    };
    let Some(value) = metrics.get(metric_name) else {
        bail!("JSON output did not contain metric `{metric_name}`");
    };

    value
        .as_f64()
        .with_context(|| format!("metric `{metric_name}` was not a number"))
}

fn parse_regex_metric(output: &str, regex: Option<&Regex>) -> Result<f64> {
    let regex = regex.context("regex metric parsing requires a configured pattern")?;
    let captures = regex
        .captures(output)
        .context("regex did not match command output")?;
    let value = captures
        .get(1)
        .context("regex must contain a capture group at index 1")?
        .as_str();

    value
        .parse::<f64>()
        .with_context(|| format!("failed to parse regex capture `{value}` as a number"))
}

#[cfg(test)]
mod tests {
    use regex::Regex;

    use super::{MetricFormat, parse_metric_value};

    #[test]
    fn parses_metric_lines() {
        let output = "noise\nMETRIC latency_p95=42.3\n";
        let parsed = parse_metric_value(MetricFormat::MetricLines, output, "latency_p95", None)
            .expect("metric lines should parse");
        assert_eq!(parsed, 42.3);
    }

    #[test]
    fn parses_json_metrics() {
        let output = r#"{"metrics":{"latency_p95":42.3}}"#;
        let parsed = parse_metric_value(MetricFormat::Json, output, "latency_p95", None)
            .expect("json metric should parse");
        assert_eq!(parsed, 42.3);
    }

    #[test]
    fn parses_regex_metrics() {
        let output = "p95: 42.3";
        let regex = Regex::new(r"p95:\s+([\d.]+)").expect("regex should compile");
        let parsed = parse_metric_value(MetricFormat::Regex, output, "latency_p95", Some(&regex))
            .expect("regex metric should parse");
        assert_eq!(parsed, 42.3);
    }
}
