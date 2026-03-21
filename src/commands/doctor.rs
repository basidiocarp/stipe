use anyhow::Result;
use colored::Colorize;
use spore::{Tool, discover};
use std::process::Command;

struct HealthCheck {
    name: String,
    passed: bool,
    message: String,
}

fn check_tool(tool: Tool) -> HealthCheck {
    let tool_name = format!("{:?}", tool).to_lowercase();

    match discover(tool) {
        Some(info) => {
            let cmd_name = match tool {
                Tool::Mycelium => "mycelium",
                Tool::Hyphae => "hyphae",
                Tool::Rhizome => "rhizome",
                Tool::Cap => "cap",
            };

            match Command::new(cmd_name).arg("--version").output() {
                Ok(output) => {
                    let _version = String::from_utf8_lossy(&output.stdout);
                    HealthCheck {
                        name: tool_name,
                        passed: true,
                        message: format!("v{} installed and working", info.version),
                    }
                }
                Err(_) => HealthCheck {
                    name: tool_name,
                    passed: false,
                    message: "Binary found but failed to run".to_string(),
                },
            }
        }
        None => HealthCheck {
            name: tool_name,
            passed: false,
            message: "Not installed".to_string(),
        },
    }
}

fn check_hyphae_db() -> HealthCheck {
    if let Some(data_dir) = dirs::data_dir() {
        let db_path = data_dir.join("hyphae").join("hyphae.db");
        if db_path.exists() {
            HealthCheck {
                name: "hyphae database".to_string(),
                passed: true,
                message: "Database initialized".to_string(),
            }
        } else {
            HealthCheck {
                name: "hyphae database".to_string(),
                passed: false,
                message: "Database not found (run 'stipe init' to initialize)".to_string(),
            }
        }
    } else {
        HealthCheck {
            name: "hyphae database".to_string(),
            passed: false,
            message: "Cannot determine data directory".to_string(),
        }
    }
}

fn check_claude_available() -> HealthCheck {
    let passed = Command::new("claude")
        .arg("--version")
        .output()
        .is_ok_and(|o| o.status.success());

    HealthCheck {
        name: "claude code".to_string(),
        passed,
        message: if passed {
            "Available".to_string()
        } else {
            "Not found in PATH (optional)".to_string()
        },
    }
}

pub fn run() -> Result<()> {
    println!();
    println!("{}", "Basidiocarp Ecosystem Health Check".bold());
    println!("{}", "─".repeat(75));
    println!();

    let checks = vec![
        check_tool(Tool::Mycelium),
        check_tool(Tool::Hyphae),
        check_tool(Tool::Rhizome),
        check_hyphae_db(),
        check_claude_available(),
    ];

    let mut all_passed = true;

    for check in &checks {
        if !check.passed {
            all_passed = false;
        }

        let status = if check.passed {
            format!("{} {}", "✓".green(), check.message.green())
        } else {
            format!("{} {}", "✗".red(), check.message.red())
        };

        println!("  {:<20} {}", check.name.bold(), status);
    }

    println!();

    if all_passed {
        println!("{}", "All critical checks passed!".green());
    } else {
        println!(
            "{}",
            "Some checks failed. Run 'stipe install --all' to install missing tools.".yellow()
        );
    }

    println!();

    Ok(())
}
