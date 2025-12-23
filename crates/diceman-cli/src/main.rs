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
    },
    /// Show dice notation reference
    Notation,
}

fn main() {
    let cli = Cli::parse();

    match cli.command {
        Commands::Roll { expression } => {
            match diceman::roll(&expression) {
                Ok(result) => {
                    println!("{}", result.expression);
                }
                Err(e) => {
                    eprintln!("Error: {}", e);
                    std::process::exit(1);
                }
            }
        }
        Commands::Sim { expression, n, json } => {
            match diceman::simulate(&expression, n) {
                Ok(result) => {
                    if json {
                        print_sim_json(&result);
                    } else {
                        print_sim_histogram(&expression, &result);
                    }
                }
                Err(e) => {
                    eprintln!("Error: {}", e);
                    std::process::exit(1);
                }
            }
        }
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
        let bar_width = (count as f64 / max_count as f64 * max_bar_width as f64) as usize;
        let bar: String = "█".repeat(bar_width);

        println!("{:>4}: {:40} {:5.1}%", value, bar, pct);
    }

    println!();
    println!("mean: {:.2}, std: {:.2}", result.mean, result.std_dev);
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
