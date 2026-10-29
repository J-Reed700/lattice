//! Numbers as words, the way misaki reads them through `num2words`:
//! cardinals and ordinals without "and", years in pairs, decimals digit by
//! digit after "point".

const ONES: [&str; 20] = [
    "zero",
    "one",
    "two",
    "three",
    "four",
    "five",
    "six",
    "seven",
    "eight",
    "nine",
    "ten",
    "eleven",
    "twelve",
    "thirteen",
    "fourteen",
    "fifteen",
    "sixteen",
    "seventeen",
    "eighteen",
    "nineteen",
];
const TENS: [&str; 10] = [
    "", "", "twenty", "thirty", "forty", "fifty", "sixty", "seventy", "eighty", "ninety",
];
const SCALES: [(u64, &str); 4] = [
    (1_000_000_000_000, "trillion"),
    (1_000_000_000, "billion"),
    (1_000_000, "million"),
    (1_000, "thousand"),
];

fn ones(n: u64) -> &'static str {
    ONES.get(n as usize).copied().unwrap_or("")
}

fn below_thousand(n: u64, words: &mut Vec<&'static str>) {
    if n >= 100 {
        words.push(ones(n / 100));
        words.push("hundred");
    }
    let rest = n % 100;
    if rest >= 20 {
        words.push(TENS.get((rest / 10) as usize).copied().unwrap_or(""));
        if !rest.is_multiple_of(10) {
            words.push(ones(rest % 10));
        }
    } else if rest > 0 || n == 0 {
        words.push(ones(rest));
    }
}

/// `1234` → `one thousand two hundred thirty four`.
pub(super) fn cardinal(n: u64) -> Vec<&'static str> {
    let mut words = Vec::new();
    let mut rest = n;
    for (scale, name) in SCALES {
        if rest >= scale {
            below_thousand(rest / scale, &mut words);
            words.push(name);
            rest %= scale;
        }
    }
    if rest > 0 || words.is_empty() {
        below_thousand(rest, &mut words);
    }
    words
}

/// `23` → `twenty third`.
pub(super) fn ordinal(n: u64) -> Vec<String> {
    let mut words: Vec<String> = cardinal(n).into_iter().map(str::to_string).collect();
    if let Some(last) = words.last_mut() {
        *last = match last.as_str() {
            "one" => "first".into(),
            "two" => "second".into(),
            "three" => "third".into(),
            "five" => "fifth".into(),
            "eight" => "eighth".into(),
            "nine" => "ninth".into(),
            "twelve" => "twelfth".into(),
            word => match word.strip_suffix('y') {
                Some(stem) => format!("{stem}ieth"),
                None => format!("{word}th"),
            },
        };
    }
    words
}

/// `1990` → `nineteen ninety`; `2005` → `two thousand five`; `1905` →
/// `nineteen oh five`.
pub(super) fn year(n: u64) -> Vec<&'static str> {
    let (high, low) = (n / 100, n % 100);
    if high == 0 || (high % 10 == 0 && low < 10) || high >= 100 {
        return cardinal(n);
    }
    let mut words = cardinal(high);
    match low {
        0 => words.push("hundred"),
        1..=9 => {
            words.push("oh");
            words.push(ones(low));
        }
        _ => words.extend(cardinal(low)),
    }
    words
}

/// `3.50` → `three point five`: the fraction's digits one by one, trailing
/// zeros dropped as a float would drop them.
pub(super) fn decimal(integer: &str, fraction: &str) -> Vec<&'static str> {
    let mut words = integer
        .parse::<u64>()
        .map(cardinal)
        .unwrap_or_else(|_| vec!["zero"]);
    let fraction = fraction.trim_end_matches('0');
    words.push("point");
    if fraction.is_empty() {
        words.push("zero");
    }
    words.extend(
        fraction
            .chars()
            .filter_map(|c| c.to_digit(10))
            .map(|d| ones(u64::from(d))),
    );
    words
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_cardinals_without_and() {
        assert_eq!(cardinal(0).join(" "), "zero");
        assert_eq!(cardinal(15).join(" "), "fifteen");
        assert_eq!(cardinal(40).join(" "), "forty");
        assert_eq!(cardinal(123).join(" "), "one hundred twenty three");
        assert_eq!(cardinal(1_001).join(" "), "one thousand one");
        assert_eq!(
            cardinal(2_500_000).join(" "),
            "two million five hundred thousand"
        );
    }

    #[test]
    fn reads_ordinals() {
        assert_eq!(ordinal(1).join(" "), "first");
        assert_eq!(ordinal(22).join(" "), "twenty second");
        assert_eq!(ordinal(40).join(" "), "fortieth");
        assert_eq!(ordinal(100).join(" "), "one hundredth");
    }

    #[test]
    fn reads_years_in_pairs() {
        assert_eq!(year(1990).join(" "), "nineteen ninety");
        assert_eq!(year(1905).join(" "), "nineteen oh five");
        assert_eq!(year(1900).join(" "), "nineteen hundred");
        assert_eq!(year(2005).join(" "), "two thousand five");
        assert_eq!(year(2026).join(" "), "twenty twenty six");
    }

    #[test]
    fn reads_decimals_digit_by_digit() {
        assert_eq!(decimal("3", "14").join(" "), "three point one four");
        assert_eq!(decimal("0", "50").join(" "), "zero point five");
    }
}
