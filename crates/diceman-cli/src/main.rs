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
    /// Roll a Marvel Multiverse 3dMarvel check against a target
    Marvel {
        /// Number of Edge dice (reroll lowest, keep better)
        #[arg(long, default_value_t = 0)]
        edges: u32,

        /// Number of Trouble dice (reroll highest, keep worse)
        #[arg(long, default_value_t = 0)]
        troubles: u32,

        /// Target value for the check
        #[arg(long)]
        target: i64,

        /// Modifier added to the total before comparison
        #[arg(long, default_value_t = 0)]
        modifier: i64,

        /// Edge policy: reroll_lowest or chase_fantastic
        #[arg(long, default_value = "reroll_lowest")]
        policy: String,

        /// Run N trials and report rates instead of a single roll
        #[arg(long)]
        sim: Option<usize>,

        /// Seed for the simulation RNG (only meaningful with --sim)
        #[arg(long)]
        seed: Option<u64>,

        /// Output as JSON
        #[arg(long)]
        json: bool,
    },
    /// Roll a Genesys/Star Wars narrative dice pool
    Genesys {
        /// Number of Ability (green) dice
        #[arg(long, default_value_t = 0)]
        ability: u32,

        /// Number of Proficiency (yellow) dice
        #[arg(long, default_value_t = 0)]
        proficiency: u32,

        /// Number of Boost (blue) dice
        #[arg(long, default_value_t = 0)]
        boost: u32,

        /// Number of Difficulty (purple) dice
        #[arg(long, default_value_t = 0)]
        difficulty: u32,

        /// Number of Challenge (red) dice
        #[arg(long, default_value_t = 0)]
        challenge: u32,

        /// Number of Setback (black) dice
        #[arg(long, default_value_t = 0)]
        setback: u32,

        /// Number of Force (white) dice
        #[arg(long, default_value_t = 0)]
        force: u32,

        /// Output as JSON
        #[arg(long)]
        json: bool,
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
        Commands::Genesys {
            ability,
            proficiency,
            boost,
            difficulty,
            challenge,
            setback,
            force,
            json,
        } => {
            let notation = match genesys_notation(
                ability,
                proficiency,
                boost,
                difficulty,
                challenge,
                setback,
                force,
            ) {
                Ok(n) => n,
                Err(e) => {
                    eprintln!("Error: {}", e);
                    std::process::exit(1);
                }
            };
            match diceman::roll(&notation) {
                Ok(result) => {
                    if json {
                        println!("{}", serde_json::to_string_pretty(&result).unwrap());
                    } else {
                        println!("{}", result.expression);
                    }
                }
                Err(e) => {
                    eprintln!("Error: {}", e);
                    std::process::exit(1);
                }
            }
        }
        Commands::Marvel {
            edges,
            troubles,
            target,
            modifier,
            policy,
            sim,
            seed,
            json,
        } => {
            let policy = match parse_policy(&policy) {
                Ok(p) => p,
                Err(e) => {
                    eprintln!("Error: {}", e);
                    std::process::exit(1);
                }
            };
            if let Some(n) = sim {
                let result = if let Some(seed) = seed {
                    diceman::simulate_marvel_seeded(
                        edges, troubles, target, modifier, policy, n, seed,
                    )
                } else {
                    diceman::simulate_marvel(edges, troubles, target, modifier, policy, n)
                };
                match result {
                    Ok(result) => {
                        if json {
                            println!("{}", serde_json::to_string_pretty(&result).unwrap());
                        } else {
                            print_marvel_sim(edges, troubles, policy, &result);
                        }
                    }
                    Err(e) => {
                        eprintln!("Error: {}", e);
                        std::process::exit(1);
                    }
                }
            } else {
                match diceman::roll_marvel(edges, troubles, target, modifier, policy) {
                    Ok(check) => {
                        if json {
                            println!("{}", serde_json::to_string_pretty(&check).unwrap());
                        } else {
                            println!("{}", check.expression);
                            println!("{}", format_marvel_verdict(&check));
                            println!("{}", format_marvel_context(&check));
                        }
                    }
                    Err(e) => {
                        eprintln!("Error: {}", e);
                        std::process::exit(1);
                    }
                }
            }
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

/// Build a `&`-joined narrative notation string from Genesys pool counts.
///
/// Groups are emitted in spec §2.10 order (ability, proficiency, boost,
/// difficulty, challenge, setback, force) regardless of flag argument order;
/// zero-count groups are omitted. Errors if every count is zero.
#[allow(clippy::too_many_arguments)]
fn genesys_notation(
    ability: u32,
    proficiency: u32,
    boost: u32,
    difficulty: u32,
    challenge: u32,
    setback: u32,
    force: u32,
) -> Result<String, String> {
    let groups = [
        (ability, "Ability"),
        (proficiency, "Proficiency"),
        (boost, "Boost"),
        (difficulty, "Difficulty"),
        (challenge, "Challenge"),
        (setback, "Setback"),
        (force, "Force"),
    ];
    let notation: Vec<String> = groups
        .iter()
        .filter(|(count, _)| *count > 0)
        .map(|(count, name)| format!("{}d{}", count, name))
        .collect();

    if notation.is_empty() {
        return Err("at least one die is required (e.g. --ability 2)".to_string());
    }

    Ok(notation.join("&"))
}

fn parse_policy(s: &str) -> Result<diceman::EdgePolicy, String> {
    match s {
        "reroll_lowest" => Ok(diceman::EdgePolicy::RerollLowest),
        "chase_fantastic" => Ok(diceman::EdgePolicy::ChaseFantastic),
        other => Err(format!(
            "unknown policy '{}', expected reroll_lowest or chase_fantastic",
            other
        )),
    }
}

fn format_marvel_verdict(check: &diceman::MarvelCheck) -> String {
    let base = if check.outcome.auto_fail {
        "Auto-fail"
    } else if check.success {
        "Success"
    } else {
        "Failure"
    };
    let fantastic = match check.fantastic {
        Some(diceman::Fantastic::Success) => " (Fantastic success)",
        Some(diceman::Fantastic::Failure) => " (Fantastic failure)",
        None => "",
    };
    format!("{}{}", base, fantastic)
}

fn format_marvel_context(check: &diceman::MarvelCheck) -> String {
    let sign = if check.modifier >= 0 { "+" } else { "" };
    format!(
        "Total {} vs target {} (modifier {}{})",
        check.outcome.total, check.target, sign, check.modifier
    )
}

fn format_marvel_rates(result: &diceman::MarvelSimResult) -> String {
    let lines = [
        ("success rate:", result.success_rate),
        ("fantastic success:", result.fantastic_success_rate),
        ("fantastic failure:", result.fantastic_failure_rate),
        ("auto-fail:", result.auto_fail_rate),
        ("M shown:", result.m_shown_rate),
    ];
    lines
        .iter()
        .map(|(label, rate)| format!("{:<20} {:>6.2}%", label, rate * 100.0))
        .collect::<Vec<_>>()
        .join("\n")
}

fn print_marvel_sim(
    edges: u32,
    troubles: u32,
    policy: diceman::EdgePolicy,
    result: &diceman::MarvelSimResult,
) {
    let policy_str = match policy {
        diceman::EdgePolicy::RerollLowest => "reroll_lowest",
        diceman::EdgePolicy::ChaseFantastic => "chase_fantastic",
    };
    println!(
        "Marvel (n={}, target={}, modifier={}, edges={}, troubles={}, policy={})",
        result.n, result.target, result.modifier, edges, troubles, policy_str
    );
    println!();
    println!("{}", format_marvel_rates(result));
    println!();
    print_sim_histogram("3dMarvel total", &result.total);
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

fn print_notation_reference() {
    println!(
        r#"DICE NOTATION REFERENCE

BASIC ROLLS
  NdS       Roll N dice with S sides (2d6, 1d20)
  dS        Roll 1 die (d20 = 1d20)
  d%        Percentile die (d100)
  dF        Fudge die (-1, 0, +1)

DIGIT DICE
  Dnn       Roll one dS per digit, read the dice as digits of one number
            (digits must all match: D66, D88, D444)

  Examples:
  D66       Roll 2d6, read as a two-digit number (3, 5 -> 35)
  D666      Roll 3d6, read as a three-digit number (1, 4, 6 -> 146)
  D88       Roll 2d8 (7, 8 -> 78)

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

MARVEL MULTIVERSE
  3dMarvel  Roll 3d6; the middle die is the Marvel die. Its 1-face
            shows as M (counts as 6 for the total) except on a raw
            1 / M / 1, where M reverts to 1 and the check auto-fails
            (total 3). Totals range 3-18.
            Example: 3dMarvel[3, 3, 4] = 10

  M shown   "Fantastic" — success or failure depends on the target.

  eN        Edge: reroll the lowest-ranked die N times, keeping the
            better result. The Marvel die ranks highest.
  tN        Trouble: reroll the highest-ranked die N times, keeping
            the worse result. Edge and Trouble cancel 1:1 (e2t1 == e1).

  The chase-fantastic Edge policy (reroll the Marvel die until M)
  is API/CLI only; there is no notation token for it.

  Use `diceman marvel --edges N --target T` for target/verdict checks;
  `roll 3dMarvel` only shows the expression, not a target verdict.

GENESYS / STAR WARS NARRATIVE DICE
  Roll a pool of narrative dice by joining full die-word groups with `&`:
  NdAbility, NdProficiency, NdBoost, NdDifficulty, NdChallenge, NdSetback,
  NdForce. Faces cancel pairwise into net facts instead of summing to a
  number, so narrative rolls cannot appear in arithmetic (2dAbility + 2
  errors with "dice result is not numeric").

  Quote narrative expressions in a shell: unquoted `&` backgrounds the
  command. Prefer `diceman genesys --ability 2 --difficulty 1`, the
  documented entry point, over raw notation.

  Faces  S success, A advantage, Tr triumph, F failure, Th threat,
         De despair, L light, Dk dark, - blank
  Net facts: successes = (S + Tr) - (F + De), advantages = A - Th.
  Triumph/despair counts are always reported, never cancelled; light/dark
  never cancel each other.

  Example: 2dAbility&1dDifficulty[SA, AA | Th] = 1 success, 2 advantages

MODIFIER ORDER
  Modifiers apply: reroll -> explode -> keep/drop -> success count
  Marvel Edge/Trouble first cancel 1:1, then the remaining Marvel rerolls apply.
  Example: 4d6r!kh3 rerolls 1s, explodes 6s, then keeps highest 3"#
    );
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

    #[test]
    fn parse_policy_known_values() {
        assert_eq!(
            parse_policy("reroll_lowest").unwrap(),
            diceman::EdgePolicy::RerollLowest
        );
        assert_eq!(
            parse_policy("chase_fantastic").unwrap(),
            diceman::EdgePolicy::ChaseFantastic
        );
    }

    #[test]
    fn parse_policy_rejects_unknown() {
        assert!(parse_policy("bogus").is_err());
        assert!(parse_policy("").is_err());
        assert!(parse_policy("RerollLowest").is_err());
    }

    #[test]
    fn marvel_command_requires_target() {
        let result = Cli::try_parse_from(["diceman", "marvel"]);
        assert!(matches!(
            result,
            Err(err) if err.kind() == clap::error::ErrorKind::MissingRequiredArgument
        ));
        assert!(Cli::try_parse_from(["diceman", "marvel", "--target", "12"]).is_ok());
    }

    fn marvel_check(
        total: i64,
        auto_fail: bool,
        m_shown: bool,
        target: i64,
        modifier: i64,
        success: bool,
        fantastic: Option<diceman::Fantastic>,
    ) -> diceman::MarvelCheck {
        diceman::MarvelCheck {
            outcome: diceman::MarvelOutcome {
                total,
                auto_fail,
                m_shown,
            },
            target,
            modifier,
            success,
            fantastic,
            expression: String::new(),
        }
    }

    #[test]
    fn marvel_verdict_success_no_fantastic() {
        let check = marvel_check(12, false, false, 10, 0, true, None);
        assert_eq!(format_marvel_verdict(&check), "Success");
    }

    #[test]
    fn marvel_verdict_success_with_fantastic() {
        let check = marvel_check(
            12,
            false,
            true,
            10,
            0,
            true,
            Some(diceman::Fantastic::Success),
        );
        assert_eq!(format_marvel_verdict(&check), "Success (Fantastic success)");
    }

    #[test]
    fn marvel_verdict_failure_with_fantastic() {
        let check = marvel_check(
            8,
            false,
            true,
            10,
            0,
            false,
            Some(diceman::Fantastic::Failure),
        );
        assert_eq!(format_marvel_verdict(&check), "Failure (Fantastic failure)");
    }

    #[test]
    fn marvel_verdict_auto_fail_without_m() {
        let check = marvel_check(3, true, false, 10, 0, false, None);
        assert_eq!(format_marvel_verdict(&check), "Auto-fail");
    }

    #[test]
    fn marvel_verdict_auto_fail_with_m_shown() {
        // Raw [1, M, 1]: auto_fail AND m_shown both true, fantastic is Failure.
        let check = marvel_check(
            3,
            true,
            true,
            10,
            0,
            false,
            Some(diceman::Fantastic::Failure),
        );
        assert_eq!(
            format_marvel_verdict(&check),
            "Auto-fail (Fantastic failure)"
        );
    }

    #[test]
    fn marvel_context_formats_modifier_sign() {
        let check = marvel_check(12, false, false, 10, 3, true, None);
        assert_eq!(
            format_marvel_context(&check),
            "Total 12 vs target 10 (modifier +3)"
        );
    }

    #[test]
    fn marvel_context_negative_modifier() {
        let check = marvel_check(12, false, false, 10, -2, true, None);
        assert_eq!(
            format_marvel_context(&check),
            "Total 12 vs target 10 (modifier -2)"
        );
    }

    fn marvel_sim_result(
        success_rate: f64,
        fantastic_success_rate: f64,
        fantastic_failure_rate: f64,
        auto_fail_rate: f64,
        m_shown_rate: f64,
    ) -> diceman::MarvelSimResult {
        let mut distribution = std::collections::HashMap::new();
        distribution.insert(10, 500);
        distribution.insert(11, 500);
        diceman::MarvelSimResult {
            n: 1000,
            target: 15,
            modifier: 0,
            success_rate,
            fantastic_success_rate,
            fantastic_failure_rate,
            auto_fail_rate,
            m_shown_rate,
            total: diceman::SimResult {
                distribution,
                min: 3,
                max: 18,
                mean: 10.5,
                std_dev: 2.0,
                n: 1000,
            },
        }
    }

    #[test]
    fn marvel_rates_formats_all_lines() {
        let result = marvel_sim_result(0.6234, 0.1012, 0.05, 0.0046, 0.1667);
        let out = format_marvel_rates(&result);
        assert!(out.contains("success rate:"));
        assert!(out.contains("62.34%"));
        assert!(out.contains("fantastic success:"));
        assert!(out.contains("10.12%"));
        assert!(out.contains("fantastic failure:"));
        assert!(out.contains("5.00%"));
        assert!(out.contains("auto-fail:"));
        assert!(out.contains("0.46%"));
        assert!(out.contains("M shown:"));
        assert!(out.contains("16.67%"));
        assert_eq!(out.lines().count(), 5);
    }

    #[test]
    fn marvel_sim_json_shape() {
        let result = diceman::simulate_marvel_seeded(
            0,
            0,
            15,
            0,
            diceman::EdgePolicy::RerollLowest,
            1000,
            42,
        )
        .expect("seeded sim");
        let value = serde_json::to_value(&result).expect("serialize");
        let obj = value.as_object().expect("object");
        for key in [
            "n",
            "target",
            "modifier",
            "success_rate",
            "fantastic_success_rate",
            "fantastic_failure_rate",
            "auto_fail_rate",
            "m_shown_rate",
            "total",
        ] {
            assert!(obj.contains_key(key), "missing top-level key: {}", key);
        }
        let total = obj
            .get("total")
            .expect("total")
            .as_object()
            .expect("total object");
        for key in ["n", "min", "max", "mean", "std_dev", "distribution"] {
            assert!(total.contains_key(key), "missing total key: {}", key);
        }
    }

    #[test]
    fn marvel_check_json_shape() {
        let check =
            diceman::roll_marvel(0, 0, 10, 0, diceman::EdgePolicy::RerollLowest).expect("roll");
        let value = serde_json::to_value(&check).expect("serialize");
        let obj = value.as_object().expect("object");
        for key in [
            "outcome",
            "target",
            "modifier",
            "success",
            "fantastic",
            "expression",
        ] {
            assert!(obj.contains_key(key), "missing key: {}", key);
        }
        let outcome = obj
            .get("outcome")
            .expect("outcome")
            .as_object()
            .expect("outcome object");
        for key in ["total", "auto_fail", "m_shown"] {
            assert!(outcome.contains_key(key), "missing outcome key: {}", key);
        }
    }

    #[test]
    fn genesys_notation_rejects_zero_flags() {
        assert!(genesys_notation(0, 0, 0, 0, 0, 0, 0).is_err());
    }

    #[test]
    fn genesys_notation_single_ability() {
        assert_eq!(genesys_notation(2, 0, 0, 0, 0, 0, 0).unwrap(), "2dAbility");
    }

    #[test]
    fn genesys_notation_single_proficiency() {
        assert_eq!(
            genesys_notation(0, 1, 0, 0, 0, 0, 0).unwrap(),
            "1dProficiency"
        );
    }

    #[test]
    fn genesys_notation_single_boost() {
        assert_eq!(genesys_notation(0, 0, 3, 0, 0, 0, 0).unwrap(), "3dBoost");
    }

    #[test]
    fn genesys_notation_single_difficulty() {
        assert_eq!(
            genesys_notation(0, 0, 0, 2, 0, 0, 0).unwrap(),
            "2dDifficulty"
        );
    }

    #[test]
    fn genesys_notation_single_challenge() {
        assert_eq!(
            genesys_notation(0, 0, 0, 0, 1, 0, 0).unwrap(),
            "1dChallenge"
        );
    }

    #[test]
    fn genesys_notation_single_setback() {
        assert_eq!(genesys_notation(0, 0, 0, 0, 0, 1, 0).unwrap(), "1dSetback");
    }

    #[test]
    fn genesys_notation_single_force() {
        assert_eq!(genesys_notation(0, 0, 0, 0, 0, 0, 1).unwrap(), "1dForce");
    }

    #[test]
    fn genesys_notation_joins_in_spec_group_order() {
        // Spec §2.10 group order: ability, proficiency, boost, difficulty,
        // challenge, setback, force — independent of flag argument order.
        assert_eq!(
            genesys_notation(2, 1, 1, 2, 1, 1, 1).unwrap(),
            "2dAbility&1dProficiency&1dBoost&2dDifficulty&1dChallenge&1dSetback&1dForce"
        );
    }

    #[test]
    fn genesys_notation_omits_zero_groups() {
        assert_eq!(
            genesys_notation(2, 0, 0, 1, 0, 0, 0).unwrap(),
            "2dAbility&1dDifficulty"
        );
    }

    #[test]
    fn genesys_command_requires_at_least_one_flag() {
        // Zero dice is a runtime error (all flags default to 0), not a clap
        // parse error, so this must parse successfully.
        let result = Cli::try_parse_from(["diceman", "genesys"]);
        assert!(result.is_ok());
    }

    #[test]
    fn genesys_command_accepts_ability_flag() {
        let result = Cli::try_parse_from(["diceman", "genesys", "--ability", "2"]);
        assert!(result.is_ok());
    }

    #[test]
    fn genesys_roll_json_shape() {
        let notation = genesys_notation(2, 0, 0, 1, 0, 0, 0).unwrap();
        let result = diceman::roll(&notation).expect("roll");
        let value = serde_json::to_value(&result).expect("serialize");
        let obj = value.as_object().expect("object");
        for key in ["outcome", "dice", "expression", "annotations"] {
            assert!(obj.contains_key(key), "missing key: {}", key);
        }
        let outcome = obj.get("outcome").expect("outcome");
        let symbols = outcome
            .get("Symbols")
            .expect("Symbols variant")
            .as_object()
            .expect("Symbols object");
        for key in [
            "successes",
            "advantages",
            "triumphs",
            "despairs",
            "light",
            "dark",
        ] {
            assert!(symbols.contains_key(key), "missing symbols key: {}", key);
        }
    }
}
