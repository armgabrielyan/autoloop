use std::path::{Path, PathBuf};

use anyhow::Result;

use crate::cli::InstallTool;

#[derive(Debug, Clone)]
pub struct GeneratedFile {
    pub relative_path: PathBuf,
    pub contents: String,
}

#[derive(Debug, Clone, Copy)]
struct ActionSpec {
    name: &'static str,
    description: &'static str,
    shared_body: &'static str,
}

const ACTIONS: &[ActionSpec] = &[
    ActionSpec {
        name: "autoloop-init",
        description: "Use when the user wants to bootstrap autoloop in the current workspace.",
        shared_body: include_str!("../integrations/_shared/init.md"),
    },
    ActionSpec {
        name: "autoloop-baseline",
        description: "Use when the user wants to record or refresh the autoloop baseline metric.",
        shared_body: include_str!("../integrations/_shared/baseline.md"),
    },
    ActionSpec {
        name: "autoloop-doctor",
        description: "Use when the user wants to verify or repair `.autoloop/config.toml`.",
        shared_body: include_str!("../integrations/_shared/doctor.md"),
    },
    ActionSpec {
        name: "autoloop-run",
        description: "Use when the user wants an autonomous bounded autoloop optimization run.",
        shared_body: include_str!("../integrations/_shared/run.md"),
    },
    ActionSpec {
        name: "autoloop-status",
        description: "Use when the user wants the current autoloop state or progress summary.",
        shared_body: include_str!("../integrations/_shared/status.md"),
    },
    ActionSpec {
        name: "autoloop-learn",
        description: "Use when the user wants to refresh `.autoloop/learnings.md` from recorded history.",
        shared_body: include_str!("../integrations/_shared/learn.md"),
    },
    ActionSpec {
        name: "autoloop-finalize",
        description: "Use when the user wants review branches created from committed kept experiments.",
        shared_body: include_str!("../integrations/_shared/finalize.md"),
    },
];

pub fn generate(workspace_root: &Path, tool: InstallTool) -> Result<Vec<GeneratedFile>> {
    Ok(match tool {
        InstallTool::Codex => generate_codex(workspace_root),
        InstallTool::ClaudeCode => generate_claude(workspace_root),
        InstallTool::Cursor => generate_cursor(workspace_root),
        InstallTool::Opencode => generate_opencode(workspace_root),
        InstallTool::GeminiCli => generate_gemini(workspace_root),
        InstallTool::Generic => generate_generic(workspace_root),
    })
}

pub fn context_path_for_tool(tool: InstallTool) -> &'static str {
    match tool {
        InstallTool::Codex | InstallTool::Cursor | InstallTool::Opencode => "AGENTS.md",
        InstallTool::ClaudeCode => "CLAUDE.md",
        InstallTool::GeminiCli => "GEMINI.md",
        InstallTool::Generic => "program.md",
    }
}

fn generate_codex(workspace_root: &Path) -> Vec<GeneratedFile> {
    let mut files = vec![GeneratedFile {
        relative_path: PathBuf::from("AGENTS.md"),
        contents: render_context("AGENTS.md", workspace_root),
    }];

    for action in ACTIONS {
        let skill_dir = PathBuf::from(".agents").join("skills").join(action.name);
        files.push(GeneratedFile {
            relative_path: skill_dir.join("SKILL.md"),
            contents: render_skill_md(action.name, action.description, &render_skill_body(action)),
        });
        files.push(GeneratedFile {
            relative_path: skill_dir.join("agents").join("openai.yaml"),
            contents: render_openai_yaml(action.name, action.description),
        });
    }

    files
}

fn generate_cursor(workspace_root: &Path) -> Vec<GeneratedFile> {
    let mut files = vec![GeneratedFile {
        relative_path: PathBuf::from("AGENTS.md"),
        contents: render_context("AGENTS.md", workspace_root),
    }];

    for action in ACTIONS {
        let skill_dir = PathBuf::from(".cursor").join("skills").join(action.name);
        files.push(GeneratedFile {
            relative_path: skill_dir.join("SKILL.md"),
            contents: render_skill_md(action.name, action.description, &render_skill_body(action)),
        });
    }

    files
}

fn generate_opencode(workspace_root: &Path) -> Vec<GeneratedFile> {
    let mut files = vec![GeneratedFile {
        relative_path: PathBuf::from("AGENTS.md"),
        contents: render_context("AGENTS.md", workspace_root),
    }];

    for action in ACTIONS {
        let skill_dir = PathBuf::from(".opencode").join("skills").join(action.name);
        files.push(GeneratedFile {
            relative_path: skill_dir.join("SKILL.md"),
            contents: render_opencode_skill_md(
                action.name,
                action.description,
                &render_skill_body(action),
            ),
        });
    }

    files
}

fn generate_gemini(workspace_root: &Path) -> Vec<GeneratedFile> {
    let mut files = vec![GeneratedFile {
        relative_path: PathBuf::from("GEMINI.md"),
        contents: render_context("GEMINI.md", workspace_root),
    }];

    for action in ACTIONS {
        let skill_dir = PathBuf::from(".gemini").join("skills").join(action.name);
        files.push(GeneratedFile {
            relative_path: skill_dir.join("SKILL.md"),
            contents: render_skill_md(action.name, action.description, &render_skill_body(action)),
        });
    }

    files
}

fn generate_claude(workspace_root: &Path) -> Vec<GeneratedFile> {
    let mut files = vec![GeneratedFile {
        relative_path: PathBuf::from("CLAUDE.md"),
        contents: render_context("CLAUDE.md", workspace_root),
    }];

    for action in ACTIONS {
        files.push(GeneratedFile {
            relative_path: PathBuf::from(".claude")
                .join("commands")
                .join(format!("{}.md", action.name)),
            contents: render_claude_command(action),
        });
    }

    files
}

fn generate_generic(workspace_root: &Path) -> Vec<GeneratedFile> {
    vec![GeneratedFile {
        relative_path: PathBuf::from("program.md"),
        contents: render_generic_program(workspace_root),
    }]
}

fn render_context(filename: &str, workspace_root: &Path) -> String {
    format!(
        "# Autoloop\n\nThis workspace uses the local `autoloop` CLI as the source of truth for autonomous experiment loops.\n\n## Workspace root\n\n{}/\n\n## Installed context\n\n- This file: `{}`\n- State lives under `.autoloop/`\n- Prefer `autoloop` CLI output with `--json` when a structured decision is needed\n- The installed names like `autoloop-run` and `autoloop-init` are agent wrappers, not native `autoloop` CLI subcommands\n\n## Primary workflow\n\n- `autoloop-init` bootstraps autoloop in the repo and prepares `.autoloop/config.toml`.\n- `autoloop-doctor` verifies or repairs `.autoloop/config.toml` when setup is incomplete or broken.\n- `autoloop-baseline` records the baseline metric once config is healthy.\n- `autoloop-run` is the main autonomous loop entrypoint.\n- `autoloop-status` reports current or historical progress.\n- `autoloop-learn` refreshes `.autoloop/learnings.md`.\n- `autoloop-finalize` creates review branches from committed kept experiments.\n\n## Rules\n\n- Treat `autoloop-run` as permission to initialize autoloop, verify and repair config, record a baseline, run a bounded loop, end the session, and refresh learnings.\n- Default bounded runs to 5 experiments when the user does not provide a different limit.\n- Do not manually edit `.autoloop/state.json`, `.autoloop/last_eval.json`, or `.autoloop/experiments.jsonl`.\n- Let `autoloop` own experiment bookkeeping, eval verdicts, keep/discard state, and finalize branches.\n- Ask the user only when blocked by missing information, unsafe ambiguity, or a genuine external dependency.\n",
        workspace_root.display(),
        filename
    )
}

fn render_generic_program(workspace_root: &Path) -> String {
    let mut sections = vec![render_context("program.md", workspace_root)];
    for action in ACTIONS {
        sections.push(format!(
            "## {}\n\n{}\n",
            title_case(action.name),
            render_skill_body(action)
        ));
    }
    sections.join("\n")
}

fn render_skill_md(name: &str, description: &str, body: &str) -> String {
    format!("---\nname: {name}\ndescription: {description}\n---\n\n{body}\n")
}

fn render_opencode_skill_md(name: &str, description: &str, body: &str) -> String {
    format!(
        "---\nname: {name}\ndescription: {description}\ncompatibility: opencode\n---\n\n{body}\n"
    )
}

fn render_openai_yaml(name: &str, description: &str) -> String {
    format!(
        "interface:\n  display_name: \"{}\"\n  short_description: \"{}\"\n  default_prompt: \"Use ${} for this workspace.\"\n",
        title_case(name),
        description,
        name
    )
}

fn render_claude_command(action: &ActionSpec) -> String {
    format!(
        "# Autoloop Command: `{}`\n\n{}\n",
        action.name,
        render_skill_body(action)
    )
}

fn render_skill_body(action: &ActionSpec) -> String {
    format!(
        "Use the local `autoloop` CLI as the source of truth for this workflow action.\n\nThese installed names are agent wrappers, not native `autoloop` CLI subcommands. A wrapper may call multiple `autoloop` commands and edit normal project files under the hood.\n\n## Required action\n\n1. Work from the current workspace root.\n2. Use `autoloop` commands, preferring `--json` when structured output is needed.\n3. Return important CLI output faithfully.\n4. Do not manually edit `.autoloop/state.json`, `.autoloop/last_eval.json`, or `.autoloop/experiments.jsonl`.\n5. If the `autoloop` executable is unavailable, stop and tell the user to install or build it.\n\n## Shared contract reference\n\n{}\n",
        action.shared_body.trim()
    )
}

fn title_case(value: &str) -> String {
    value
        .split('-')
        .map(capitalize)
        .collect::<Vec<_>>()
        .join(" ")
}

fn capitalize(value: &str) -> String {
    let mut chars = value.chars();
    match chars.next() {
        Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
        None => String::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::{context_path_for_tool, generate};
    use crate::cli::InstallTool;
    use std::fs;
    use std::path::{Path, PathBuf};
    use std::time::{SystemTime, UNIX_EPOCH};

    fn temp_dir(label: &str) -> PathBuf {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("time should be monotonic enough for tests")
            .as_nanos();
        let dir =
            std::env::temp_dir().join(format!("autoloop-{label}-{}-{unique}", std::process::id()));
        fs::create_dir_all(&dir).expect("failed to create temp dir");
        dir
    }

    fn write_files(root: &Path, files: &[super::GeneratedFile]) {
        for file in files {
            let path = root.join(&file.relative_path);
            if let Some(parent) = path.parent() {
                fs::create_dir_all(parent).expect("failed to create parent directory");
            }
            fs::write(path, &file.contents).expect("failed to write generated file");
        }
    }

    fn read(path: &Path) -> String {
        fs::read_to_string(path).expect("failed to read file")
    }

    #[test]
    fn codex_generation_creates_expected_files() {
        let out = temp_dir("codex");
        let files = generate(&out, InstallTool::Codex).expect("generation should succeed");
        write_files(&out, &files);

        assert_eq!(context_path_for_tool(InstallTool::Codex), "AGENTS.md");
        assert!(out.join("AGENTS.md").exists());
        assert!(out.join(".agents/skills/autoloop-run/SKILL.md").exists());
        assert!(out.join(".agents/skills/autoloop-init/SKILL.md").exists());
        assert!(out.join(".agents/skills/autoloop-doctor/SKILL.md").exists());
        assert!(
            out.join(".agents/skills/autoloop-run/agents/openai.yaml")
                .exists()
        );

        let context = read(&out.join("AGENTS.md"));
        assert!(context.contains("autoloop-run"));
        assert!(context.contains("autoloop-doctor"));
        assert!(context.contains(&out.display().to_string()));
        assert!(context.contains("agent wrappers, not native `autoloop` CLI subcommands"));
    }

    #[test]
    fn claude_generation_creates_expected_files() {
        let out = temp_dir("claude");
        let files = generate(&out, InstallTool::ClaudeCode).expect("generation should succeed");
        write_files(&out, &files);

        assert_eq!(context_path_for_tool(InstallTool::ClaudeCode), "CLAUDE.md");
        assert!(out.join("CLAUDE.md").exists());
        assert!(out.join(".claude/commands/autoloop-run.md").exists());
        assert!(out.join(".claude/commands/autoloop-doctor.md").exists());
        assert!(out.join(".claude/commands/autoloop-finalize.md").exists());

        let command = read(&out.join(".claude/commands/autoloop-run.md"));
        assert!(command.contains("autoloop doctor --json"));
        assert!(command.contains("autoloop doctor --fix --json"));
        assert!(command.contains("default to 5 experiments"));
        assert!(command.contains("Do not edit `.git/info/exclude`"));
        assert!(command.contains("setup-only"));
    }

    #[test]
    fn gemini_generation_creates_expected_files() {
        let out = temp_dir("gemini");
        let files = generate(&out, InstallTool::GeminiCli).expect("generation should succeed");
        write_files(&out, &files);

        assert!(out.join("GEMINI.md").exists());
        assert!(out.join(".gemini/skills/autoloop-run/SKILL.md").exists());
    }

    #[test]
    fn cursor_generation_creates_expected_files() {
        let out = temp_dir("cursor");
        let files = generate(&out, InstallTool::Cursor).expect("generation should succeed");
        write_files(&out, &files);

        assert!(out.join("AGENTS.md").exists());
        assert!(out.join(".cursor/skills/autoloop-status/SKILL.md").exists());
    }

    #[test]
    fn opencode_generation_creates_expected_files() {
        let out = temp_dir("opencode");
        let files = generate(&out, InstallTool::Opencode).expect("generation should succeed");
        write_files(&out, &files);

        let skill = read(&out.join(".opencode/skills/autoloop-run/SKILL.md"));
        assert!(skill.contains("compatibility: opencode"));
    }

    #[test]
    fn generic_generation_creates_program() {
        let out = temp_dir("generic");
        let files = generate(&out, InstallTool::Generic).expect("generation should succeed");
        write_files(&out, &files);

        let program = read(&out.join("program.md"));
        assert!(program.contains("## Autoloop Run"));
        assert!(program.contains("## Autoloop Doctor"));
        assert!(program.contains("autoloop-finalize"));
        assert!(program.contains("## What Helped"));
    }
}
