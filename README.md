# Niriksh

> **Track your AI usage, tokens, and costs — right from your terminal.**

Niriksh is a lightweight **AI usage tracking tool built in Rust**. It helps developers monitor token usage and cost across various AI-powered developer tools from a single terminal-based interface.

The goal is simple: **make AI usage visible, understandable, and easy to track.**

## ✨ What it does

* 📊 Tracks AI tool usage
* 🔢 Tracks input, output, and total tokens
* 💰 Provides AI usage/cost insights
* 📈 Helps understand usage trends
* 🖥️ Provides a terminal-based UI
* 💾 Stores usage data locally using SQLite

## 🛠️ Tech Stack

* **Rust** — Core application
* **Ratatui** — Terminal UI
* **Crossterm** — Terminal handling
* **Tokio** — Async runtime
* **Reqwest** — HTTP/API communication
* **SQLite / rusqlite** — Local data storage
* **Serde** — Serialization/deserialization
* **Clap** — CLI

## 🚀 Getting Started

```bash
git clone https://github.com/krishnamhn009/niriksh.git
cd niriksh
cargo run
```

Build a release version:

```bash
cargo build --release
```

## 🤝 Open Source & Contributions

Niriksh is an open-source project, and **Rust developers are welcome to contribute!**

If you love writing Rust, building developer tools, working with terminal UIs, or simply want to learn and experiment with Rust, we'd love to have you involved.

You can contribute by:

* 🦀 Writing Rust
* ✨ Adding new features
* 🔌 Adding support for more AI tools
* 🐛 Fixing bugs
* ⚡ Improving performance
* 🎨 Improving the TUI
* 📚 Improving documentation

Fork the repository, create a branch, make your changes, and open a Pull Request.

**Let's build a useful AI developer tool together — one Rust commit at a time. 🦀**

## ⭐ Project Status

Niriksh is currently under active development.

If you find it useful, consider giving the project a ⭐ on GitHub.

---

**Repository:** https://github.com/krishnamhn009/niriksh
