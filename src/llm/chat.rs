use crate::agent::{
    ApplyOptions, OperationLog, OperationPlan, Pipeline, Policy, PolicyDecision, ValidationResult,
};
use crate::llm::config::LlmConfig;
use crate::llm::mock::MockLlmProvider;
use crate::llm::parser::LlmIntentParser;
use crate::llm::provider::{LlmError, LlmProvider};
use std::io::BufRead as _;
use std::io::Write as _;
use std::path::Path;
use std::sync::Arc;

#[cfg(feature = "network")]
use crate::llm::openai::OpenAiCompatibleProvider;

pub struct ChatCommand {
    provider: Arc<dyn LlmProvider>,
    config: LlmConfig,
    auto_approve: bool,
}

impl ChatCommand {
    pub fn new(config_path: Option<&Path>, auto_approve: bool) -> Result<Self, LlmError> {
        let config = match config_path {
            Some(path) => LlmConfig::load_from(path)?,
            None => LlmConfig::load()?,
        };

        let provider = create_provider(&config)?;

        Ok(Self {
            provider,
            config,
            auto_approve,
        })
    }

    pub fn run(&self) -> Result<(), anyhow::Error> {
        let stdin = std::io::stdin();
        let mut stdout = std::io::stdout();

        println!(
            "AI folder-intelligence chat (provider: {}, model: {})",
            self.config.provider, self.config.model
        );
        println!("Type 'quit' or 'exit' to end session.\n");

        let default_scope = std::env::current_dir()
            .map_err(|e| anyhow::anyhow!("Failed to get current directory: {}", e))?;

        let parser = LlmIntentParser::new(default_scope, self.provider.clone());

        let mut session = ChatSession {
            parser,
            pending: None,
            last_log: None,
            auto_approve: self.auto_approve,
        };

        loop {
            print!("> ");
            stdout.flush()?;

            let mut input = String::new();
            let n = stdin.lock().read_line(&mut input)?;
            if n == 0 {
                println!();
                break;
            }

            let input = input.trim();
            if input.is_empty() {
                continue;
            }

            let lc = input.to_lowercase();
            if lc == "quit" || lc == "exit" {
                println!("Goodbye!");
                break;
            }

            if session.pending.is_some() {
                match lc.as_str() {
                    "execute" | "yes" | "y" | "apply" => {
                        if let Err(e) = session.execute() {
                            eprintln!("Error: {}", e);
                        }
                        continue;
                    }
                    "cancel" | "no" | "n" => {
                        session.cancel();
                        continue;
                    }
                    "show" | "plan" => {
                        session.display_plan();
                        continue;
                    }
                    "undo" => {
                        if let Err(e) = session.undo() {
                            eprintln!("Error: {}", e);
                        }
                        continue;
                    }
                    "help" => {
                        print_chat_help();
                        continue;
                    }
                    _ => {}
                }
            }

            if let Err(e) = session.handle_request(input) {
                eprintln!("Error: {}", e);
            }
        }

        Ok(())
    }
}

fn create_provider(config: &LlmConfig) -> Result<Arc<dyn LlmProvider>, LlmError> {
    match config.provider.as_str() {
        "openai" => {
            #[cfg(feature = "network")]
            {
                let api_key = config.resolve_api_key()?;
                Ok(Arc::new(OpenAiCompatibleProvider::new(
                    config.base_url.clone(),
                    api_key,
                    config.model.clone(),
                    config.timeout(),
                )))
            }
            #[cfg(not(feature = "network"))]
            {
                Err(LlmError::ProviderError(
                    "Network feature not enabled. Install with --features network".to_string(),
                ))
            }
        }
        "mock" => Ok(Arc::new(MockLlmProvider::with_default_organize())),
        other => Err(LlmError::ProviderError(format!(
            "Unknown provider '{}'. Supported: 'mock', 'openai'",
            other
        ))),
    }
}

struct ChatSession {
    parser: LlmIntentParser,
    pending: Option<PendingPlan>,
    last_log: Option<OperationLog>,
    auto_approve: bool,
}

struct PendingPlan {
    pipeline: Pipeline,
    plan: OperationPlan,
    validation: ValidationResult,
    decision: PolicyDecision,
}

impl ChatSession {
    fn handle_request(&mut self, request: &str) -> Result<(), anyhow::Error> {
        let intent = self.parser.parse(request)?;

        let intent_scope = match &intent.goal {
            crate::agent::Goal::Organize { scope, .. }
            | crate::agent::Goal::Reorganize { scope, .. }
            | crate::agent::Goal::Clean { scope, .. } => scope.clone(),
        };

        let pipeline = Pipeline::new(&intent_scope)
            .with_policy(Policy::default().auto_approve(self.auto_approve));

        let analysis = pipeline.analyze(&intent)?;
        let recommendation = pipeline.recommend(&intent, &analysis)?;
        let plan = pipeline.plan(&recommendation, &analysis, &intent)?;
        let validation = pipeline.validate(&plan);
        let decision = pipeline.policy_evaluate(&plan, &validation);

        display_plan_summary(&plan, &validation, &decision);

        self.pending = Some(PendingPlan {
            pipeline,
            plan,
            validation,
            decision,
        });

        match &self.pending.as_ref().unwrap().decision {
            PolicyDecision::Rejected => {
                println!("Plan was rejected by policy.");
                self.pending = None;
            }
            PolicyDecision::Approved => {
                println!("\nType 'execute' to apply, 'cancel' to discard, or type a new request.");
            }
            PolicyDecision::RequiresApproval => {
                println!("\nPlan requires approval.");
                println!("Type 'execute' to approve and apply, 'cancel' to discard, or type a new request.");
            }
        }

        Ok(())
    }

    fn execute(&mut self) -> Result<(), anyhow::Error> {
        let pending = match self.pending.take() {
            Some(p) => p,
            None => {
                println!("No pending plan. Type a request to generate one.");
                return Ok(());
            }
        };

        let options = ApplyOptions {
            force: false,
            dry_run: false,
        };

        let result = match pending.decision {
            PolicyDecision::Rejected => {
                eprintln!("Cannot execute a rejected plan.");
                return Ok(());
            }
            PolicyDecision::Approved => {
                pending
                    .pipeline
                    .apply(&pending.plan, &pending.validation, &options)
            }
            PolicyDecision::RequiresApproval => {
                eprintln!("Creating approval...");
                let approval = pending
                    .pipeline
                    .create_approval(&pending.plan, &pending.validation)
                    .map_err(|e| anyhow::anyhow!("Failed to create approval: {}", e))?;
                pending
                    .pipeline
                    .apply_with_approval(&pending.plan, &approval, &options)
            }
        }?;

        println!("\n✅ Execution complete.");
        println!("  Entries: {}", result.log.total_entries);
        println!("  Success: {}", result.log.success_count);
        if result.log.failure_count > 0 {
            println!("  Failures: {}", result.log.failure_count);
        }
        if result.log.skipped_count > 0 {
            println!("  Skipped: {}", result.log.skipped_count);
        }

        self.last_log = Some(result.log);

        println!("\nType 'undo' to reverse, or type a new request.");
        Ok(())
    }

    fn undo(&mut self) -> Result<(), anyhow::Error> {
        let log = match self.last_log.take() {
            Some(l) => l,
            None => {
                println!("No executed plan to undo.");
                return Ok(());
            }
        };

        let pipeline = Pipeline::default();
        let result = pipeline.undo(&log)?;

        println!("\n✅ Undo complete.");
        println!("  Operations undone: {}", result.total_undo_operations);
        if !result.conflicts.is_empty() {
            println!("  Conflicts (skipped): {}", result.conflicts.len());
        }

        Ok(())
    }

    fn cancel(&mut self) {
        self.pending = None;
        println!("Plan cancelled. Type a new request to start over.");
    }

    fn display_plan(&self) {
        if let Some(pending) = &self.pending {
            display_plan_summary(&pending.plan, &pending.validation, &pending.decision);
        } else {
            println!("No pending plan.");
        }
    }
}

fn display_plan_summary(
    plan: &OperationPlan,
    validation: &ValidationResult,
    decision: &PolicyDecision,
) {
    use crate::agent::{FileSystemOperation, ValidationStatus};

    let moves = plan
        .operations
        .iter()
        .filter(|op| matches!(op, FileSystemOperation::Move { .. }))
        .count();
    let creates = plan
        .operations
        .iter()
        .filter(|op| matches!(op, FileSystemOperation::CreateDir { .. }))
        .count();
    let deletes = plan
        .operations
        .iter()
        .filter(|op| matches!(op, FileSystemOperation::Delete { .. }))
        .count();

    println!("\n=== Plan Summary ===");
    println!("  Scope: {}", plan.scope.display());
    println!("  Operations: {}", plan.operations.len());
    println!("    Moves: {}", moves);
    println!("    Dirs created: {}", creates);
    println!("    Deletes: {}", deletes);
    println!(
        "  Estimated impact: {} files moved, {} dirs created",
        plan.estimated_impact.files_moved, plan.estimated_impact.dirs_created
    );
    println!(
        "  Validation: {} executable, {} invalid, {} conflicts",
        validation.executable_operations, validation.summary.invalid, validation.summary.conflicts
    );

    if validation.has_conflicts {
        println!("  CONFLICTS DETECTED");
    }
    if validation.has_blocked {
        println!("  BLOCKED operations present (use --force to override)");
    }
    if plan.dry_run {
        println!("  [DRY RUN — not executable]");
    }

    match decision {
        PolicyDecision::Rejected => println!("  Policy: ❌ Rejected"),
        PolicyDecision::Approved => println!("  Policy: ✅ Approved"),
        PolicyDecision::RequiresApproval => println!("  Policy: ⚠️  Requires approval"),
    }

    let invalid_ops: Vec<_> = validation
        .validated_operations
        .iter()
        .filter(|op| !op.status.is_valid())
        .collect();
    if !invalid_ops.is_empty() {
        println!("  Reasons:");
        for op in invalid_ops {
            let status_str = match &op.status {
                ValidationStatus::Invalid(msg) => msg,
                ValidationStatus::Conflict(msg) => msg,
                _ => "unknown",
            };
            println!("    - {}: {}", op.operation.description(), status_str);
        }
    }

    println!();
}

fn print_chat_help() {
    println!("Commands:");
    println!("  <natural language request>  Parse and generate a plan");
    println!("  execute                     Apply the pending plan");
    println!("  cancel                      Discard the pending plan");
    println!("  show                        Show the pending plan summary");
    println!("  undo                        Undo the last execution");
    println!("  help                        Show this help");
    println!("  quit / exit                 Exit the chat");
    println!();
}
