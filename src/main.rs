/*!
 * AI Context Management Tool - Main CLI (Simplified)
 *
 * シンプル化されたCLIエントリーポイント
 */

use aicm::agents::claude::ClaudeAgent;
use aicm::agents::cline::ClineAgent;
use aicm::agents::codex::CodexAgent;
use aicm::agents::cursor::CursorAgent;
use aicm::agents::github::GitHubAgent;
use aicm::config::{error::ConfigError, loader::ConfigLoader};
use aicm::types::{AIContextConfig, GeneratedFile};
use anyhow::Result;
use clap::{Parser, Subcommand};
use std::path::Path;
use tokio::fs;

#[derive(Parser)]
#[command(name = "aicm")]
#[command(about = "AI Context Management Tool - 複数のAIツール用設定ファイルを統一管理")]
#[command(version)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// プロジェクトを初期化（設定ファイルとドキュメントディレクトリを作成）
    Init,
    /// AI用設定ファイルを生成
    Generate {
        /// 特定のエージェントのみ生成
        #[arg(long)]
        agent: Option<String>,
        /// 設定ファイルのパス
        #[arg(short, long)]
        config: Option<String>,
    },
    /// 設定ファイルを検証
    Validate {
        /// 設定ファイルのパス
        #[arg(short, long)]
        config: Option<String>,
    },
}

const CONFIG_FILE: &str = "ai-context.yaml";

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Commands::Init => handle_init().await,
        Commands::Generate { agent, config } => handle_generate(agent, config).await,
        Commands::Validate { config } => handle_validate(config).await,
    }
}

/// init コマンドの処理
async fn handle_init() -> Result<()> {
    println!("プロジェクトを初期化します...");

    // 設定ファイルが既に存在するかチェック
    if Path::new(CONFIG_FILE).exists() {
        println!("⚠️  {}は既に存在します", CONFIG_FILE);
    } else {
        // デフォルト設定ファイルを作成
        let config = ConfigLoader::create_default(CONFIG_FILE).await?;
        println!("✅ {}を作成しました", CONFIG_FILE);

        // ドキュメントディレクトリを作成
        create_docs_directory(&config).await?;
    }

    Ok(())
}

/// generate コマンドの処理
async fn handle_generate(agent_filter: Option<String>, config_path: Option<String>) -> Result<()> {
    let config_file = config_path.as_deref().unwrap_or(CONFIG_FILE);
    println!("コンテキストファイルを生成します: {}", config_file);

    // 設定ファイルを読み込み
    let config = load_config_from_path(config_file).await?;

    // 有効なエージェントを取得
    let enabled_agents = get_enabled_agents(&config, agent_filter);

    if enabled_agents.is_empty() {
        println!("❌ 有効なエージェントがありません");
        println!("💡 ai-context.yaml の agents セクションでエージェントを有効にしてください");
        return Ok(());
    }

    // 各エージェントのファイルを生成
    for agent_name in enabled_agents {
        match generate_agent_files(&config, &agent_name).await {
            Ok(files) => {
                for file in files {
                    write_generated_file(&file).await?;
                    println!("📄 {}", file.path);
                }
            }
            Err(e) => {
                println!("❌ {}の生成でエラーが発生しました: {}", agent_name, e);
            }
        }
    }

    println!("✅ コンテキストファイルの生成が完了しました");
    Ok(())
}

/// validate コマンドの処理
async fn handle_validate(config_path: Option<String>) -> Result<()> {
    let config_file = config_path.as_deref().unwrap_or(CONFIG_FILE);
    println!("設定ファイルを検証します: {}", config_file);

    match load_config_from_path(config_file).await {
        Ok(config) => {
            println!("✅ 設定ファイルは有効です");

            // 基本情報を表示
            println!("  バージョン: {}", config.version);
            println!("  出力モード: {:?}", config.output_mode);
            println!("  ドキュメントディレクトリ: {}", config.base_docs_dir);

            // 有効なエージェントを表示
            let enabled = config.enabled_agents();
            if enabled.is_empty() {
                println!("  有効なエージェント: なし");
            } else {
                println!("  有効なエージェント: {}", enabled.join(", "));
            }

            // ドキュメントディレクトリの存在確認
            if Path::new(&config.base_docs_dir).exists() {
                println!("  ドキュメントディレクトリ: 存在します");
            } else {
                println!(
                    "  ⚠️  ドキュメントディレクトリが存在しません: {}",
                    config.base_docs_dir
                );
            }
        }
        Err(e) => {
            println!("❌ 設定ファイルの検証でエラーが発生しました: {}", e);
            // テスト環境では exit を呼ばずに Result::Err を返す
            if cfg!(test) {
                return Err(anyhow::anyhow!(
                    "設定ファイルの検証でエラーが発生しました: {}",
                    e
                ));
            } else {
                std::process::exit(1);
            }
        }
    }

    Ok(())
}

/// 指定されたパスから設定ファイルを読み込み
async fn load_config_from_path(config_path: &str) -> Result<AIContextConfig, ConfigError> {
    if !Path::new(config_path).exists() {
        return Err(ConfigError::FileNotFound {
            path: config_path.to_string(),
        });
    }

    ConfigLoader::load(config_path).await
}

/// ドキュメントディレクトリを作成
async fn create_docs_directory(config: &AIContextConfig) -> Result<()> {
    let docs_dir = Path::new(&config.base_docs_dir);

    if docs_dir.exists() {
        println!(
            "⚠️  ドキュメントディレクトリは既に存在します: {}",
            config.base_docs_dir
        );
    } else {
        fs::create_dir_all(docs_dir).await?;
        println!(
            "✅ ドキュメントディレクトリを作成しました: {}",
            config.base_docs_dir
        );

        // README.mdを作成
        let readme_content = create_readme_content();
        let readme_path = docs_dir.join("README.md");
        fs::write(readme_path, readme_content).await?;
        println!("📄 {}/README.md", config.base_docs_dir);
    }

    Ok(())
}

/// README.mdの内容を作成
fn create_readme_content() -> &'static str {
    r#"# AI Context Management - ドキュメント

このディレクトリに Markdown ファイルを配置してください。

## 使い方

1. **任意の .md ファイルを作成**
   - ファイル名は自由に設定できます
   - サブディレクトリも使用可能です

2. **コンテンツを記述**
   - プロジェクトのルール
   - コーディング規約
   - アーキテクチャ情報
   - など

3. **ファイルを生成**
   ```bash
   aicm generate
   ```

## ファイル例

```
docs/
├── README.md
├── coding-rules.md
├── project-info.md
└── architecture/
    ├── overview.md
    └── patterns.md
```

全ての .md ファイルが自動的に検出され、AI ツール用の設定ファイルに統合されます。
"#
}

/// 有効なエージェントのリストを取得
fn get_enabled_agents(config: &AIContextConfig, filter: Option<String>) -> Vec<String> {
    let all_enabled = config.enabled_agents();

    match filter {
        Some(agent_name) => {
            if all_enabled.contains(&agent_name) {
                vec![agent_name]
            } else {
                println!("❌ エージェント '{}' は有効ではありません", agent_name);
                println!("💡 有効なエージェント: {}", all_enabled.join(", "));
                vec![]
            }
        }
        None => all_enabled,
    }
}

/// 指定されたエージェントのファイルを生成
async fn generate_agent_files(
    config: &AIContextConfig,
    agent_name: &str,
) -> Result<Vec<GeneratedFile>> {
    match agent_name {
        "cursor" => {
            let agent = CursorAgent::new(config.clone());
            agent.generate().await
        }
        "claude" => {
            let agent = ClaudeAgent::new(config.clone());
            agent.generate().await
        }
        "github" => {
            let agent = GitHubAgent::new(config.clone());
            agent.generate().await
        }
        "cline" => {
            let agent = ClineAgent::new(config.clone());
            agent.generate().await
        }
        "codex" => {
            let agent = CodexAgent::new(config.clone());
            agent.generate().await
        }
        _ => Err(anyhow::anyhow!("未対応のエージェント: {}", agent_name)),
    }
}

/// 生成されたファイルを書き込み
async fn write_generated_file(file: &GeneratedFile) -> Result<()> {
    // ディレクトリを作成
    if let Some(parent) = Path::new(&file.path).parent() {
        fs::create_dir_all(parent).await?;
    }

    // ファイルを書き込み
    fs::write(&file.path, &file.content).await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use aicm::types::AgentConfigTrait;
    use tempfile::tempdir;
    use tokio::fs;

    #[test]
    fn test_create_readme_content() {
        let content = create_readme_content();
        assert!(content.contains("AI Context Management"));
        assert!(content.contains("aicm generate"));
        assert!(content.contains("docs/"));
    }

    #[test]
    fn test_get_enabled_agents_with_filter() {
        let mut config = AIContextConfig::default();
        config.agents.cursor = aicm::types::CursorConfig::Simple(true);
        config.agents.claude = aicm::types::ClaudeConfig::Simple(true);

        // フィルターなし
        let all_agents = get_enabled_agents(&config, None);
        assert_eq!(all_agents.len(), 2);
        assert!(all_agents.contains(&"cursor".to_string()));
        assert!(all_agents.contains(&"claude".to_string()));

        // 有効なエージェントでフィルター
        let filtered = get_enabled_agents(&config, Some("cursor".to_string()));
        assert_eq!(filtered, vec!["cursor"]);

        // 無効なエージェントでフィルター
        let invalid = get_enabled_agents(&config, Some("invalid".to_string()));
        assert!(invalid.is_empty());
    }

    #[test]
    fn test_get_enabled_agents_no_agents() {
        let config = AIContextConfig::default();
        let agents = get_enabled_agents(&config, None);
        assert!(agents.is_empty());
    }

    #[tokio::test]
    async fn test_load_config_from_path_valid() {
        let temp_dir = tempdir().unwrap();
        let config_path = temp_dir.path().join("custom-config.yaml");

        let test_config_content = r#"
version: "1.0"
output_mode: split
base_docs_dir: "./custom-docs"
agents:
  cursor: true
  claude: true
"#;

        fs::write(&config_path, test_config_content).await.unwrap();

        let config = load_config_from_path(&config_path.to_string_lossy())
            .await
            .unwrap();
        assert_eq!(config.version, "1.0");
        assert_eq!(config.base_docs_dir, "./custom-docs");
        assert!(config.agents.cursor.is_enabled());
        assert!(config.agents.claude.is_enabled());
    }

    #[tokio::test]
    async fn test_load_config_from_path_not_found() {
        let result = load_config_from_path("/nonexistent/config.yaml").await;
        assert!(result.is_err());

        if let Err(ConfigError::FileNotFound { path }) = result {
            assert_eq!(path, "/nonexistent/config.yaml");
        } else {
            panic!("Expected FileNotFound error");
        }
    }

    #[tokio::test]
    async fn test_load_config_from_path_invalid_yaml() {
        let temp_dir = tempdir().unwrap();
        let config_path = temp_dir.path().join("invalid.yaml");

        let invalid_yaml = r#"
version: 1.0
invalid_yaml: [
"#;

        fs::write(&config_path, invalid_yaml).await.unwrap();

        let result = load_config_from_path(&config_path.to_string_lossy()).await;
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), ConfigError::YamlError { .. }));
    }

    #[tokio::test]
    async fn test_load_config_from_path_with_default_file() {
        // デフォルトファイルパスでのテスト
        let result = load_config_from_path(CONFIG_FILE).await;

        // デフォルトファイルが存在する場合は成功、存在しない場合はFileNotFoundエラー
        match result {
            Ok(config) => {
                // ファイルが存在する場合は正常に読み込まれることを確認
                assert!(!config.version.is_empty());
                assert!(!config.base_docs_dir.is_empty());
            }
            Err(ConfigError::FileNotFound { path }) => {
                // ファイルが存在しない場合はFileNotFoundエラーが返される
                assert_eq!(path, CONFIG_FILE);
            }
            Err(e) => {
                panic!("Unexpected error type: {:?}", e);
            }
        }
    }

    #[tokio::test]
    async fn test_handle_validate_with_custom_config() {
        let temp_dir = tempdir().unwrap();
        let config_path = temp_dir.path().join("validate-test-config.yaml");

        let test_config_content = r#"
version: "1.0"
output_mode: split
base_docs_dir: "./validate-docs"
agents:
  cursor: true
  claude: true
"#;

        fs::write(&config_path, test_config_content).await.unwrap();

        // handle_validate関数が正常に動作することを確認
        // 実際の出力はテストでは確認できないが、エラーが発生しないことを確認
        let result = handle_validate(Some(config_path.to_string_lossy().to_string())).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_handle_validate_with_nonexistent_config() {
        // 存在しないファイルでvalidateを実行した場合の動作確認
        // テスト環境では exit を呼ばずに Result::Err を返す
        let result = handle_validate(Some("/nonexistent/config.yaml".to_string())).await;
        assert!(result.is_err());

        let error_message = result.unwrap_err().to_string();
        assert!(error_message.contains("設定ファイルの検証でエラーが発生しました"));
    }

    #[tokio::test]
    async fn test_handle_validate_default_config() {
        // デフォルト設定でのvalidateテスト
        let result = handle_validate(None).await;

        // デフォルトファイルが存在する場合は成功、存在しない場合はエラー
        match result {
            Ok(_) => {
                // ファイルが存在する場合は正常に処理される
            }
            Err(e) => {
                // ファイルが存在しない場合はエラーが返される（テスト環境では exit しない）
                assert!(e
                    .to_string()
                    .contains("設定ファイルの検証でエラーが発生しました"));
            }
        }
    }
}
