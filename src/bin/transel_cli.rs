//! transel-cli：命令行管理工具。
//!
//! 子命令：
//! - `config show`              打印当前配置
//! - `config set target <lang>`  设置目标语言
//! - `config path`              打印配置 / 历史路径
//! - `history tail [N]`         打印最近 N 条翻译
//! - `history search <q>`       搜索翻译
//! - `history clear`            清空历史
//! - `secret set <backend>`     从 stdin 读取 API Key 并写入钥匙串
//! - `secret delete <backend>`  从钥匙串删除
//! - `backend list`             列出所有后端

use std::io::Read;

use transel::app::config::{BackendKind, Config};
use transel::app::history::HistoryStore;
use transel::app::paths;
use transel::app::secrets;

fn main() -> anyhow::Result<()> {
    let args: Vec<String> = std::env::args().collect();
    if args.len() < 2 {
        usage();
        return Ok(());
    }
    match args[1].as_str() {
        "config" => cmd_config(&args[2..])?,
        "history" => cmd_history(&args[2..])?,
        "secret" => cmd_secret(&args[2..])?,
        "backend" => cmd_backend(&args[2..])?,
        "path" => {
            println!("config: {}", paths::config_file().display());
            println!("history: {}", paths::history_file().display());
        }
        "help" | "-h" | "--help" => usage(),
        other => {
            eprintln!("未知子命令: {other}");
            usage();
            std::process::exit(2);
        }
    }
    Ok(())
}

fn usage() {
    println!("transel-cli — Transel 命令行管理");
    println!();
    println!("子命令:");
    println!("  config show                       打印当前配置");
    println!("  config set target <lang>          设置目标语言");
    println!("  config path                       打印配置/历史路径");
    println!("  history tail [N]                  打印最近 N 条翻译（默认 20）");
    println!("  history search <q>                搜索翻译");
    println!("  history clear                     清空历史");
    println!("  secret set <backend>              写入 API Key 到钥匙串（stdin）");
    println!("  secret delete <backend>           从钥匙串删除");
    println!("  backend list                      列出后端 id");
}

fn cmd_config(args: &[String]) -> anyhow::Result<()> {
    let cfg = Config::load_or_default(&paths::config_file());
    match args.first().map(|s| s.as_str()) {
        Some("show") => println!("{}", serde_json::to_string_pretty(&cfg)?),
        Some("set") => {
            if args.len() < 3 {
                anyhow::bail!("用法: config set <key> <value>");
            }
            let mut cfg = cfg;
            match (args[1].as_str(), args[2].as_str()) {
                ("target", v) => cfg.target_lang = v.to_string(),
                (k, _) => anyhow::bail!("不支持的 key: {k}"),
            }
            cfg.save(&paths::config_file())?;
            println!("已保存");
        }
        Some("path") => {
            println!("config: {}", paths::config_file().display());
            println!("history: {}", paths::history_file().display());
        }
        _ => usage(),
    }
    Ok(())
}

fn cmd_history(args: &[String]) -> anyhow::Result<()> {
    let store = HistoryStore::open(&paths::history_file(), 0)?;
    match args.first().map(|s| s.as_str()) {
        Some("tail") => {
            let n: usize = args.get(1).and_then(|s| s.parse().ok()).unwrap_or(20);
            for e in store
                .list()?
                .into_iter()
                .rev()
                .take(n)
                .collect::<Vec<_>>()
                .into_iter()
                .rev()
            {
                println!(
                    "[{}] {} -> {}",
                    e.ts.format("%Y-%m-%d %H:%M:%S"),
                    e.source,
                    e.results.first().map(|r| r.text.as_str()).unwrap_or("(无)"),
                );
            }
        }
        Some("search") => {
            let q = args.get(1).cloned().unwrap_or_default();
            for e in store.search(&q, 100)? {
                println!("[{}] {}", e.ts.format("%Y-%m-%d %H:%M:%S"), e.source);
                for r in e.results.iter().filter(|r| r.success) {
                    println!("    {}: {}", r.backend, r.text);
                }
            }
        }
        Some("clear") => {
            store.clear()?;
            println!("已清空");
        }
        _ => usage(),
    }
    Ok(())
}

fn cmd_secret(args: &[String]) -> anyhow::Result<()> {
    match args.first().map(|s| s.as_str()) {
        Some("set") => {
            let backend = args
                .get(1)
                .cloned()
                .ok_or_else(|| anyhow::anyhow!("用法: secret set <backend>"))?;
            let mut s = String::new();
            std::io::stdin().read_to_string(&mut s)?;
            let s = s.trim();
            secrets::set(&backend, s)?;
            println!("已写入钥匙串 (account={backend})");
        }
        Some("delete") => {
            let backend = args
                .get(1)
                .cloned()
                .ok_or_else(|| anyhow::anyhow!("用法: secret delete <backend>"))?;
            secrets::delete(&backend)?;
            println!("已删除");
        }
        _ => usage(),
    }
    Ok(())
}

fn cmd_backend(args: &[String]) -> anyhow::Result<()> {
    match args.first().map(|s| s.as_str()) {
        Some("list") => {
            for k in BackendKind::all() {
                println!("{}  needs_key={}", k.as_str(), k.needs_key());
            }
        }
        _ => usage(),
    }
    Ok(())
}
