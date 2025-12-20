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
