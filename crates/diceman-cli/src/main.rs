// ABOUTME: Command-line interface for the diceman dice roller.
// ABOUTME: Provides roll and simulation commands with optional JSON output.

use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(name = "diceman")]
#[command(about = "A dice notation parser and roller for TTRPGs")]
#[command(version)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Roll dice using the given expression
    Roll {
        /// Dice expression (e.g., "4d6kh3", "2d6 + 5")
        expression: String,
    },
    /// Simulate rolling dice many times
    Sim {
        /// Dice expression (e.g., "2d6")
        expression: String,

        /// Number of trials to run
        #[arg(short, long, default_value = "10000")]
        n: usize,

        /// Output as JSON
        #[arg(long)]
        json: bool,

        /// Show cumulative success probability
        #[arg(long)]
        cumulative: bool,

        /// Use "roll under" direction (implies --cumulative)
        #[arg(long)]
        lte: bool,
    },
    /// Show dice notation reference
    Notation,
}

fn main() {
    let cli = Cli::parse();

    match cli.command {
        Commands::Roll { expression } => match diceman::roll(&expression) {
            Ok(result) => {
                println!("{}", result.expression);
            }
            Err(e) => {
                eprintln!("Error: {}", e);
                std::process::exit(1);
            }
        },
        Commands::Sim {
            expression,
            n,
            json,
            cumulative,
            lte,
        } => match diceman::simulate(&expression, n) {
            Ok(result) => {
                if json {
                    print_sim_json(&result);
                } else if cumulative || lte {
                    print_side_by_side(&expression, &result, lte);
                } else {
                    print_sim_histogram(&expression, &result);
                }
            }
            Err(e) => {
                eprintln!("Error: {}", e);
                std::process::exit(1);
            }
        },
        Commands::Notation => {
            print_notation_reference();
        }
    }
}

fn print_sim_json(result: &diceman::SimResult) {
    use serde_json::json;

    let output = json!({
        "n": result.n,
        "min": result.min,
        "max": result.max,
        "mean": result.mean,
        "std_dev": result.std_dev,
        "distribution": result.distribution,
    });

    println!("{}", serde_json::to_string_pretty(&output).unwrap());
}

fn print_sim_histogram(expression: &str, result: &diceman::SimResult) {
    println!("{} (n={})", expression, result.n);
    println!();

    let outcomes = result.sorted_outcomes();
    let max_count = outcomes.iter().map(|(_, c)| *c).max().unwrap_or(1);
    let max_bar_width = 40;

    for (value, count) in outcomes {
        let pct = (count as f64 / result.n as f64) * 100.0;
        let fraction = count as f64 / max_count as f64;
        let bar = braille_bar(fraction, max_bar_width);

        println!("{:>4}: {} {:5.1}%", value, bar, pct);
    }

    println!();
    println!("mean: {:.2}, std: {:.2}", result.mean, result.std_dev);
}

fn print_side_by_side(expression: &str, result: &diceman::SimResult, lte: bool) {
    let outcomes = result.sorted_outcomes();
    let max_count = outcomes.iter().map(|(_, c)| *c).max().unwrap_or(1);

    let cumulative = if lte {
        result.cumulative_lte()
    } else {
        result.cumulative_gte()
    };
    let cum_map: std::collections::HashMap<i64, f64> = cumulative.into_iter().collect();

    let direction = if lte { "<=" } else { ">=" };

    // Header: left-align distribution label, right-align cumulative label
    // The bar area starts at column 6 (after "{:>4}: ")
    let left_header = format!("{} (n={})", expression, result.n);
    let right_header = format!("Cumulative ({} target)", direction);
    // Position right header so it aligns with the right bar area
    let header_pad = SIDE_BY_SIDE_TERM_WIDTH.saturating_sub(left_header.len() + right_header.len());
    println!(
        "{}{:>pad$}",
        left_header,
        right_header,
        pad = header_pad + right_header.len()
    );
    println!();

    for (value, count) in &outcomes {
        let dist_pct = (*count as f64 / result.n as f64) * 100.0;
        let dist_frac = *count as f64 / max_count as f64;
        let cum_pct_frac = cum_map.get(value).copied().unwrap_or(0.0);

        let row = format_side_by_side_row(
            *value,
            dist_frac,
            dist_pct,
            cum_pct_frac,
            cum_pct_frac * 100.0,
        );
        println!("{}", row);
    }

    println!();
    println!("mean: {:.2}, std: {:.2}", result.mean, result.std_dev);
}

/// Renders a horizontal bar using braille characters for 2x resolution.
/// `fraction` is 0.0..=1.0, `width` is the character width of the bar area.
fn braille_bar(fraction: f64, width: usize) -> String {
    let half_steps = (fraction.clamp(0.0, 1.0) * (width * 2) as f64).round() as usize;
    let full_chars = half_steps / 2;
    let has_half = half_steps % 2 == 1;

    let mut bar = String::with_capacity(width * 3);
    for _ in 0..full_chars {
        bar.push('⣿');
    }
    if has_half {
        bar.push('⡇');
    }
    let filled = full_chars + if has_half { 1 } else { 0 };
    for _ in filled..width {
        bar.push('⠀');
    }
    bar
}

/// Layout constants for side-by-side histogram (targeting 80-column terminal).
/// Format: "{:>4}: [bar] {:5.1}% │ [bar] {:5.1}%"
const SIDE_BY_SIDE_SEP: &str = " │ "; // 3 chars (space, box-draw, space)
const SIDE_BY_SIDE_TERM_WIDTH: usize = 80;

/// Character width available for each bar in side-by-side mode.
/// 80 - 4 (label) - 2 (": ") - 1 (space before pct) - 6 (pct) - 3 (sep) - 1 (space before pct) - 6 (pct) = 57
/// 57 / 2 = 28 each, 1 spare char goes to left bar
const SIDE_BY_SIDE_BAR_LEFT: usize = 29;
const SIDE_BY_SIDE_BAR_RIGHT: usize = 28;

/// Formats one row of the side-by-side histogram.
fn format_side_by_side_row(
    value: i64,
    dist_frac: f64,
    dist_pct: f64,
    cum_frac: f64,
    cum_pct: f64,
) -> String {
    let dist_bar = braille_bar(dist_frac, SIDE_BY_SIDE_BAR_LEFT);
    let cum_bar = braille_bar(cum_frac, SIDE_BY_SIDE_BAR_RIGHT);
    format!(
        "{:>4}: {} {:5.1}%{}{} {:5.1}%",
        value, dist_bar, dist_pct, SIDE_BY_SIDE_SEP, cum_bar, cum_pct
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn side_by_side_row_fits_80_columns() {
        let row = format_side_by_side_row(7, 1.0, 16.7, 0.583, 58.3);
        // Count display width (braille chars are 1-column wide)
        assert_eq!(
            row.chars().count(),
            SIDE_BY_SIDE_TERM_WIDTH,
            "row was {} chars: {}",
            row.chars().count(),
            row
        );
    }

    #[test]
    fn side_by_side_row_consistent_width() {
        // Various values should all produce exactly 80 chars
        for (val, df, dp, cf, cp) in [
            (2, 0.05, 2.8, 1.0, 100.0),
            (7, 1.0, 16.7, 0.583, 58.3),
            (12, 0.05, 2.8, 0.028, 2.8),
        ] {
            let row = format_side_by_side_row(val, df, dp, cf, cp);
            assert_eq!(
                row.chars().count(),
                SIDE_BY_SIDE_TERM_WIDTH,
                "val={}: row was {} chars",
                val,
                row.chars().count()
            );
        }
    }

    #[test]
    fn braille_bar_empty() {
        let bar = braille_bar(0.0, 10);
        assert_eq!(bar.chars().count(), 10);
        assert!(bar.chars().all(|c| c == '⠀'));
    }

    #[test]
    fn braille_bar_full() {
        let bar = braille_bar(1.0, 10);
        assert_eq!(bar.chars().count(), 10);
        assert!(bar.chars().all(|c| c == '⣿'));
    }

    #[test]
    fn braille_bar_half_step() {
        // 1 out of 20 half-steps = left column of first cell only
        let bar = braille_bar(0.05, 10);
        assert_eq!(bar.chars().count(), 10);
        let chars: Vec<char> = bar.chars().collect();
        assert_eq!(chars[0], '⡇');
        assert!(chars[1..].iter().all(|&c| c == '⠀'));
    }

    #[test]
    fn braille_bar_exact_half() {
        let bar = braille_bar(0.5, 10);
        assert_eq!(bar.chars().count(), 10);
        let chars: Vec<char> = bar.chars().collect();
        // 5 full braille chars, 5 spaces
        assert!(chars[..5].iter().all(|&c| c == '⣿'));
        assert!(chars[5..].iter().all(|&c| c == '⠀'));
    }

    #[test]
    fn braille_bar_constant_width() {
        // Any fraction should produce exactly `width` characters
        for pct in 0..=100 {
            let bar = braille_bar(pct as f64 / 100.0, 20);
            assert_eq!(bar.chars().count(), 20, "failed at {}%", pct);
        }
    }
}

fn print_notation_reference() {
    println!(
        r#"DICE NOTATION REFERENCE

BASIC ROLLS
  NdS       Roll N dice with S sides (2d6, 1d20)
  dS        Roll 1 die (d20 = 1d20)
  d%        Percentile die (d100)
  dF        Fudge die (-1, 0, +1)

ARITHMETIC
  + - * /   Basic operations (2d6 + 5, (1d6 + 2) * 3)
  (...)     Grouping

KEEP AND DROP
  khN       Keep highest N dice (4d6kh3)
  klN       Keep lowest N dice (2d20kl1 for disadvantage)
  kN        Keep highest N (shorthand for khN)
  dhN       Drop highest N dice
  dlN       Drop lowest N dice (4d6dl1)

EXPLODING DICE
  !         Explode on max, new die per explosion (Roll20 style)
  !!        Compounding explode, add to same die (Shadowrun style)
  !p        Penetrating explode, -1 per explosion (HackMaster style)
  !!p       Compounding penetrating

  With conditions:
  !>N       Explode on greater than N
  !>=N      Explode on greater than or equal to N
  !<N       Explode on less than N
  !=N       Explode on equal to N

  Examples:
  1d6!      Standard exploding d6
  1d6!!     Compounding (6+6+4 shows as [16])
  1d6!p     Penetrating (6+5+3 shows as [6, 4, 2])
  1d10!>=8  Explode on 8, 9, or 10

REROLL
  r         Reroll 1s until not 1
  ro        Reroll once only
  r<N       Reroll below N
  r<=N      Reroll at or below N

  Examples:
  1d6r      Reroll 1s
  2d6r<3    Reroll 1s and 2s
  1d20ro    Reroll first 1 only

SUCCESS COUNTING
  >N        Count dice greater than N
  >=N       Count dice greater than or equal to N
  <N        Count dice less than N
  <=N       Count dice less than or equal to N
  =N        Count dice equal to N

  Examples:
  5d10>=8   World of Darkness (count 8, 9, 10)
  6d6>4     Count 5s and 6s
  8d6=6     Count only 6s

MODIFIER ORDER
  Modifiers apply: reroll -> explode -> keep/drop -> success count
  Example: 4d6r!kh3 rerolls 1s, explodes 6s, then keeps highest 3"#
    );
}
