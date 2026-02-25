use anyhow::{anyhow, Result};
use clap::{Parser, Subcommand};
use std::env;
use std::process::Command;

#[derive(Parser)]
#[command(name = "remitwise-cli")]
#[command(about = "CLI for interacting with RemitWise contracts")]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Commands for remittance split contract
    Split {
        #[command(subcommand)]
        subcommand: SplitCommands,
    },
    /// Commands for savings goals contract
    Goals {
        #[command(subcommand)]
        subcommand: GoalsCommands,
    },
    /// Commands for bill payments contract
    Bills {
        #[command(subcommand)]
        subcommand: BillsCommands,
    },
    /// Commands for insurance contract
    Insurance {
        #[command(subcommand)]
        subcommand: InsuranceCommands,
    },
}

#[derive(Subcommand)]
enum SplitCommands {
    /// Get split configuration
    GetConfig,
}

#[derive(Subcommand)]
enum GoalsCommands {
    /// List all goals
    List,
    /// Create a new goal
    Create {
        name: String,
        target_amount: u64,
        target_date: u64,
    },
}

#[derive(Subcommand)]
enum BillsCommands {
    /// List unpaid bills
    List,
    /// Pay a bill
    Pay { bill_id: u32 },
}

#[derive(Subcommand)]
enum InsuranceCommands {
    /// List policies
    List,
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Commands::Split { subcommand } => handle_split(subcommand).await,
        Commands::Goals { subcommand } => handle_goals(subcommand).await,
        Commands::Bills { subcommand } => handle_bills(subcommand).await,
        Commands::Insurance { subcommand } => handle_insurance(subcommand).await,
    }
}

async fn handle_split(subcommand: SplitCommands) -> Result<()> {
    let contract_id = get_contract_id("REMITTANCE_SPLIT_CONTRACT_ID")?;
    match subcommand {
        SplitCommands::GetConfig => {
            run_soroban_invoke(&contract_id, "get_config", &[]).await?;
        }
    }
    Ok(())
}

async fn handle_goals(subcommand: GoalsCommands) -> Result<()> {
    let contract_id = get_contract_id("SAVINGS_GOALS_CONTRACT_ID")?;
    match subcommand {
        GoalsCommands::List => {
            // Need owner address
            let owner = get_env("OWNER_ADDRESS")?;
            run_soroban_invoke(&contract_id, "get_all_goals", &[&owner]).await?;
        }
        GoalsCommands::Create {
            name,
            target_amount,
            target_date,
        } => {
            let owner = get_env("OWNER_ADDRESS")?;
            run_soroban_invoke(
                &contract_id,
                "create_goal",
                &[
                    &owner,
                    &name,
                    &target_amount.to_string(),
                    &target_date.to_string(),
                ],
            )
            .await?;
        }
    }
    Ok(())
}

async fn handle_bills(subcommand: BillsCommands) -> Result<()> {
    let contract_id = get_contract_id("BILL_PAYMENTS_CONTRACT_ID")?;
    match subcommand {
        BillsCommands::List => {
            let owner = get_env("OWNER_ADDRESS")?;
            run_soroban_invoke(&contract_id, "get_unpaid_bills", &[&owner, "0", "10"]).await?;
        }
        BillsCommands::Pay { bill_id } => {
            let owner = get_env("OWNER_ADDRESS")?;
            run_soroban_invoke(&contract_id, "pay_bill", &[&owner, &bill_id.to_string()]).await?;
        }
    }
    Ok(())
}

async fn handle_insurance(subcommand: InsuranceCommands) -> Result<()> {
    let contract_id = get_contract_id("INSURANCE_CONTRACT_ID")?;
    match subcommand {
        InsuranceCommands::List => {
            let owner = get_env("OWNER_ADDRESS")?;
            run_soroban_invoke(&contract_id, "get_active_policies", &[&owner, "0", "10"]).await?;
        }
    }
    Ok(())
}

fn get_contract_id(env_var: &str) -> Result<String> {
    env::var(env_var).map_err(|_| anyhow!("Environment variable {} not set", env_var))
}

fn get_env(env_var: &str) -> Result<String> {
    env::var(env_var).map_err(|_| anyhow!("Environment variable {} not set", env_var))
}

async fn run_soroban_invoke(contract_id: &str, function: &str, args: &[&str]) -> Result<()> {
    let mut cmd = Command::new("soroban");
    cmd.arg("contract")
        .arg("invoke")
        .arg("--id")
        .arg(contract_id)
        .arg("--")
        .arg(function);
    for arg in args {
        cmd.arg(arg);
    }
    let output = cmd.output()?;
    if output.status.success() {
        println!("{}", String::from_utf8_lossy(&output.stdout));
    } else {
        eprintln!("{}", String::from_utf8_lossy(&output.stderr));
        return Err(anyhow!("Command failed"));
    }
    Ok(())
}
