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
    /// Commands for family wallet contract
    #[command(name = "family-wallet")]
    FamilyWallet {
        #[command(subcommand)]
        subcommand: FamilyWalletCommands,
    },
    /// Commands for orchestrator contract
    Orchestrator {
        #[command(subcommand)]
        subcommand: OrchestratorCommands,
    },
    /// Commands for emergency killswitch contract
    Killswitch {
        #[command(subcommand)]
        subcommand: KillswitchCommands,
    },
}

// ---------------------------------------------------------------------------
// Existing subcommand enums
// ---------------------------------------------------------------------------

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

// ---------------------------------------------------------------------------
// Family Wallet subcommands
// ---------------------------------------------------------------------------

#[derive(Subcommand)]
enum FamilyWalletCommands {
    /// Initialize the family wallet
    Init {
        /// Owner address
        owner: String,
    },
    /// Add a member to the wallet
    AddMember {
        /// Admin address performing the action
        admin: String,
        /// New member address
        member: String,
        /// Role: 1=Owner, 2=Admin, 3=Member, 4=Viewer
        role: u32,
        /// Spending limit in stroops (0 = unlimited)
        spending_limit: i128,
    },
    /// Update a member's spending limit
    UpdateLimit {
        /// Caller address (must be admin or owner)
        caller: String,
        /// Member address to update
        member: String,
        /// New spending limit in stroops (0 = unlimited)
        new_limit: i128,
    },
    /// Check whether an address is within its spending limit
    CheckLimit {
        /// Address to check
        caller: String,
        /// Amount in stroops
        amount: i128,
    },
    /// Configure multisig for a transaction type
    ConfigureMultisig {
        /// Caller address (must be owner)
        caller: String,
        /// Transaction type: 1=LargeWithdrawal, 2=SplitConfigChange, 3=RoleChange,
        ///                   4=EmergencyTransfer, 5=PolicyCancellation, 6=RegularWithdrawal
        tx_type: u32,
        /// Required signature threshold
        threshold: u32,
        /// Comma-separated list of signer addresses
        signers: String,
        /// Spending limit for this tx type in stroops
        spending_limit: i128,
    },
    /// Propose a withdrawal transaction
    ProposeWithdrawal {
        /// Proposer address
        proposer: String,
        /// Token contract address
        token: String,
        /// Recipient address
        recipient: String,
        /// Amount in stroops
        amount: i128,
    },
    /// Sign a pending multisig transaction
    SignTransaction {
        /// Signer address
        signer: String,
        /// Transaction ID to sign
        tx_id: u64,
    },
    /// Activate emergency mode
    ActivateEmergency {
        /// Caller address (must be admin or owner)
        caller: String,
    },
    /// Deactivate emergency mode
    DeactivateEmergency {
        /// Caller address (must be admin or owner)
        caller: String,
    },
    /// Get all pending transactions (paginated)
    ListPending {
        /// Starting cursor (0 for first page)
        #[arg(default_value = "0")]
        cursor: u64,
        /// Page size
        #[arg(default_value = "20")]
        limit: u32,
    },
    /// Get a specific member's details
    GetMember {
        /// Member address
        member: String,
    },
    /// Archive old executed transactions
    ArchiveTransactions,
}

// ---------------------------------------------------------------------------
// Orchestrator subcommands
// ---------------------------------------------------------------------------

#[derive(Subcommand)]
enum OrchestratorCommands {
    /// Initialize the orchestrator with dependency contract addresses
    Init {
        /// Caller address (becomes owner)
        caller: String,
        /// Family wallet contract address
        family_wallet: String,
        /// Remittance split contract address
        remittance_split: String,
        /// Savings goals contract address
        savings_goals: String,
        /// Bill payments contract address
        bill_payments: String,
        /// Insurance contract address
        insurance: String,
    },
    /// Execute a remittance flow with replay protection
    ExecuteFlow {
        /// Executor address
        executor: String,
        /// Total amount in stroops
        amount: i128,
        /// Replay-protection nonce (use get-nonce to retrieve current value)
        nonce: u64,
        /// Request deadline as Unix timestamp
        deadline: u64,
        /// Request hash for parameter binding (compute with hash-request)
        request_hash: u64,
    },
    /// Get the current replay-protection nonce for an address
    GetNonce {
        /// Address to query
        address: String,
    },
    /// Get execution statistics
    GetStats,
    /// Get the audit log (paginated)
    GetAuditLog {
        /// Starting index (0-based)
        #[arg(default_value = "0")]
        from_index: u32,
        /// Number of entries to return (max 50)
        #[arg(default_value = "20")]
        limit: u32,
    },
    /// Get the contract version
    GetVersion,
}

// ---------------------------------------------------------------------------
// Emergency Killswitch subcommands
// ---------------------------------------------------------------------------

#[derive(Subcommand)]
enum KillswitchCommands {
    /// Initialize the killswitch with an admin address
    Initialize {
        /// Admin address
        admin: String,
    },
    /// Transfer admin rights to a new address
    TransferAdmin {
        /// New admin address
        new_admin: String,
    },
    /// Globally pause all contracts
    Pause,
    /// Globally unpause all contracts (respects scheduled unpause time)
    Unpause,
    /// Schedule a future unpause at a specific Unix timestamp
    ScheduleUnpause {
        /// Unix timestamp when unpause becomes effective
        time: u64,
    },
    /// Check whether the killswitch is globally paused
    IsPaused,
    /// Pause a specific module
    PauseModule {
        /// Module identifier (max 9 chars)
        module_id: String,
    },
    /// Unpause a specific module
    UnpauseModule {
        /// Module identifier (max 9 chars)
        module_id: String,
    },
    /// Pause a specific function within a module
    PauseFunction {
        /// Module identifier (max 9 chars)
        module_id: String,
        /// Function name (max 9 chars)
        func: String,
    },
    /// Unpause a specific function within a module
    UnpauseFunction {
        /// Module identifier (max 9 chars)
        module_id: String,
        /// Function name (max 9 chars)
        func: String,
    },
    /// Check whether a specific function is paused
    IsFunctionPaused {
        /// Module identifier (max 9 chars)
        module_id: String,
        /// Function name (max 9 chars)
        func: String,
    },
}

// ---------------------------------------------------------------------------
// Entry point
// ---------------------------------------------------------------------------

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Commands::Split { subcommand } => handle_split(subcommand).await,
        Commands::Goals { subcommand } => handle_goals(subcommand).await,
        Commands::Bills { subcommand } => handle_bills(subcommand).await,
        Commands::Insurance { subcommand } => handle_insurance(subcommand).await,
        Commands::FamilyWallet { subcommand } => handle_family_wallet(subcommand).await,
        Commands::Orchestrator { subcommand } => handle_orchestrator(subcommand).await,
        Commands::Killswitch { subcommand } => handle_killswitch(subcommand).await,
    }
}

// ---------------------------------------------------------------------------
// Existing handlers
// ---------------------------------------------------------------------------

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

// ---------------------------------------------------------------------------
// Family Wallet handler
// ---------------------------------------------------------------------------

async fn handle_family_wallet(subcommand: FamilyWalletCommands) -> Result<()> {
    let contract_id = get_contract_id("FAMILY_WALLET_CONTRACT_ID")?;

    match subcommand {
        FamilyWalletCommands::Init { owner } => {
            // initial_members is an empty Vec; pass as XDR-encoded empty vec
            run_soroban_invoke(&contract_id, "init", &[&owner, "[]"]).await?;
        }

        FamilyWalletCommands::AddMember {
            admin,
            member,
            role,
            spending_limit,
        } => {
            run_soroban_invoke(
                &contract_id,
                "add_member",
                &[
                    &admin,
                    &member,
                    &role.to_string(),
                    &spending_limit.to_string(),
                ],
            )
            .await?;
        }

        FamilyWalletCommands::UpdateLimit {
            caller,
            member,
            new_limit,
        } => {
            run_soroban_invoke(
                &contract_id,
                "update_spending_limit",
                &[&caller, &member, &new_limit.to_string()],
            )
            .await?;
        }

        FamilyWalletCommands::CheckLimit { caller, amount } => {
            run_soroban_invoke(
                &contract_id,
                "check_spending_limit",
                &[&caller, &amount.to_string()],
            )
            .await?;
        }

        FamilyWalletCommands::ConfigureMultisig {
            caller,
            tx_type,
            threshold,
            signers,
            spending_limit,
        } => {
            // signers is a comma-separated list; convert to JSON array for soroban CLI
            let signer_vec: Vec<&str> = signers.split(',').map(|s| s.trim()).collect();
            let signers_json = format!("[{}]", signer_vec.join(","));
            run_soroban_invoke(
                &contract_id,
                "configure_multisig",
                &[
                    &caller,
                    &tx_type.to_string(),
                    &threshold.to_string(),
                    &signers_json,
                    &spending_limit.to_string(),
                ],
            )
            .await?;
        }

        FamilyWalletCommands::ProposeWithdrawal {
            proposer,
            token,
            recipient,
            amount,
        } => {
            run_soroban_invoke(
                &contract_id,
                "withdraw",
                &[&proposer, &token, &recipient, &amount.to_string()],
            )
            .await?;
        }

        FamilyWalletCommands::SignTransaction { signer, tx_id } => {
            run_soroban_invoke(
                &contract_id,
                "sign_transaction",
                &[&signer, &tx_id.to_string()],
            )
            .await?;
        }

        FamilyWalletCommands::ActivateEmergency { caller } => {
            run_soroban_invoke(&contract_id, "activate_emergency_mode", &[&caller]).await?;
        }

        FamilyWalletCommands::DeactivateEmergency { caller } => {
            run_soroban_invoke(&contract_id, "deactivate_emergency_mode", &[&caller]).await?;
        }

        FamilyWalletCommands::ListPending { cursor, limit } => {
            run_soroban_invoke(
                &contract_id,
                "get_pending_transactions",
                &[&cursor.to_string(), &limit.to_string()],
            )
            .await?;
        }

        FamilyWalletCommands::GetMember { member } => {
            run_soroban_invoke(&contract_id, "get_member", &[&member]).await?;
        }

        FamilyWalletCommands::ArchiveTransactions => {
            run_soroban_invoke(&contract_id, "archive_old_transactions", &[]).await?;
        }
    }

    Ok(())
}

// ---------------------------------------------------------------------------
// Orchestrator handler
// ---------------------------------------------------------------------------

async fn handle_orchestrator(subcommand: OrchestratorCommands) -> Result<()> {
    let contract_id = get_contract_id("ORCHESTRATOR_CONTRACT_ID")?;

    match subcommand {
        OrchestratorCommands::Init {
            caller,
            family_wallet,
            remittance_split,
            savings_goals,
            bill_payments,
            insurance,
        } => {
            run_soroban_invoke(
                &contract_id,
                "init",
                &[
                    &caller,
                    &family_wallet,
                    &remittance_split,
                    &savings_goals,
                    &bill_payments,
                    &insurance,
                ],
            )
            .await?;
        }

        OrchestratorCommands::ExecuteFlow {
            executor,
            amount,
            nonce,
            deadline,
            request_hash,
        } => {
            run_soroban_invoke(
                &contract_id,
                "execute_remittance_flow",
                &[
                    &executor,
                    &amount.to_string(),
                    &nonce.to_string(),
                    &deadline.to_string(),
                    &request_hash.to_string(),
                ],
            )
            .await?;
        }

        OrchestratorCommands::GetNonce { address } => {
            run_soroban_invoke(&contract_id, "get_nonce", &[&address]).await?;
        }

        OrchestratorCommands::GetStats => {
            run_soroban_invoke(&contract_id, "get_execution_stats", &[]).await?;
        }

        OrchestratorCommands::GetAuditLog { from_index, limit } => {
            run_soroban_invoke(
                &contract_id,
                "get_audit_log",
                &[&from_index.to_string(), &limit.to_string()],
            )
            .await?;
        }

        OrchestratorCommands::GetVersion => {
            run_soroban_invoke(&contract_id, "get_version", &[]).await?;
        }
    }

    Ok(())
}

// ---------------------------------------------------------------------------
// Emergency Killswitch handler
// ---------------------------------------------------------------------------

async fn handle_killswitch(subcommand: KillswitchCommands) -> Result<()> {
    let contract_id = get_contract_id("KILLSWITCH_CONTRACT_ID")?;

    match subcommand {
        KillswitchCommands::Initialize { admin } => {
            run_soroban_invoke(&contract_id, "initialize", &[&admin]).await?;
        }

        KillswitchCommands::TransferAdmin { new_admin } => {
            run_soroban_invoke(&contract_id, "transfer_admin", &[&new_admin]).await?;
        }

        KillswitchCommands::Pause => {
            run_soroban_invoke(&contract_id, "pause", &[]).await?;
        }

        KillswitchCommands::Unpause => {
            run_soroban_invoke(&contract_id, "unpause", &[]).await?;
        }

        KillswitchCommands::ScheduleUnpause { time } => {
            run_soroban_invoke(&contract_id, "schedule_unpause", &[&time.to_string()]).await?;
        }

        KillswitchCommands::IsPaused => {
            run_soroban_invoke(&contract_id, "is_paused", &[]).await?;
        }

        KillswitchCommands::PauseModule { module_id } => {
            run_soroban_invoke(&contract_id, "pause_module", &[&module_id]).await?;
        }

        KillswitchCommands::UnpauseModule { module_id } => {
            run_soroban_invoke(&contract_id, "unpause_module", &[&module_id]).await?;
        }

        KillswitchCommands::PauseFunction { module_id, func } => {
            run_soroban_invoke(&contract_id, "pause_function", &[&module_id, &func]).await?;
        }

        KillswitchCommands::UnpauseFunction { module_id, func } => {
            run_soroban_invoke(&contract_id, "unpause_function", &[&module_id, &func]).await?;
        }

        KillswitchCommands::IsFunctionPaused { module_id, func } => {
            run_soroban_invoke(&contract_id, "is_function_paused", &[&module_id, &func]).await?;
        }
    }

    Ok(())
}

// ---------------------------------------------------------------------------
// Shared helpers
// ---------------------------------------------------------------------------

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
