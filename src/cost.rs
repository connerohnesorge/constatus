/// Estimate API cost from token counts and model name.
///
/// Pricing per million tokens (USD), approximate as of 2025:
///   Opus 4:   input $15, output $75, cache_read $1.50
///   Sonnet 4: input $3,  output $15, cache_read $0.30
///   Haiku 3.5: input $0.80, output $4, cache_read $0.08

struct Pricing {
    input_per_m: f64,
    output_per_m: f64,
    cache_read_per_m: f64,
}

const OPUS: Pricing = Pricing {
    input_per_m: 15.0,
    output_per_m: 75.0,
    cache_read_per_m: 1.50,
};

const SONNET: Pricing = Pricing {
    input_per_m: 3.0,
    output_per_m: 15.0,
    cache_read_per_m: 0.30,
};

const HAIKU: Pricing = Pricing {
    input_per_m: 0.80,
    output_per_m: 4.0,
    cache_read_per_m: 0.08,
};

fn pricing_for(model: &str) -> &'static Pricing {
    let lower = model.to_lowercase();
    if lower.contains("opus") {
        &OPUS
    } else if lower.contains("haiku") {
        &HAIKU
    } else {
        &SONNET
    }
}

pub fn estimate(
    model: &str,
    input_tokens: u64,
    output_tokens: u64,
    cache_read_tokens: u64,
) -> f64 {
    let p = pricing_for(model);
    let input_cost = (input_tokens as f64 / 1_000_000.0) * p.input_per_m;
    let output_cost = (output_tokens as f64 / 1_000_000.0) * p.output_per_m;
    let cache_cost = (cache_read_tokens as f64 / 1_000_000.0) * p.cache_read_per_m;
    input_cost + output_cost + cache_cost
}

pub fn format_cost(cost: f64) -> String {
    if cost < 0.01 {
        format!("${:.4}", cost)
    } else if cost < 1.0 {
        format!("${:.3}", cost)
    } else {
        format!("${:.2}", cost)
    }
}
