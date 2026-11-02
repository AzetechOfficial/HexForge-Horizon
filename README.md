<div align="center">

# ⚡ HEXFORGE ⚡

<p align="center">
  <b>The Blazingly Fast, Memory-Safe Modding Ecosystem & Forge Fork for <a href="https://github.com/horizon-rust/horizon">Horizon</a> (Rust Minecraft)</b>
</p>

<p align="center">
  <img src="https://img.shields.io/badge/Language-Rust-orange?style=for-the-badge&logo=rust&logoColor=white" alt="Rust">
  <img src="https://img.shields.io/badge/Engine-Horizon-blueviolet?style=for-the-badge&logo=minecraft&logoColor=white" alt="Horizon Engine">
  <img src="https://img.shields.io/badge/License-MIT-blue?style=for-the-badge" alt="MIT License">
  <img src="https://img.shields.io/badge/PRs-Welcome-brightgreen?style=for-the-badge" alt="PRs Welcome">
</p>

[**Getting Started**](#-quick-start) • [**Documentation**](https://github.com/AzetechOfficial/hexforge-horizon/wiki) • [**API & Modding**](#-writing-your-first-hexforge-mod) • [**Discord**](https://discord.gg/skibiditoilet)

---
</div>

## 🌌 What is HexForge?

**HexForge** is a high-performance mod loader and API layer engineered specifically for **Horizon**—the lightning-fast, native Rust reimplementation of Minecraft Java Edition. 

While Java-based mod loaders struggle with garbage collection pauses and heavy memory overhead, HexForge taps directly into Rust's zero-cost abstractions, safety guarantees, and concurrent runtime. It gives modders deep hook capabilities, custom registries, and native performance without losing compatibility with the Horizon ecosystem.

---

## 🔥 Key Features

*   🚀 **Zero-Overhead Hook System:** Injects directly into Horizon's native execution loop with minimal latency.
*   🛡️ **Memory Safety First:** Written completely in Rust, eliminating standard JVM memory leaks and abrupt garbage collection stutters.
*   🔌 **Advanced Mod Lifecycle Management:** Hot-reload support for development environments, automatic dependency resolution, and sandboxed mod execution.
*   ⚙️ **Forge-Inspired Architecture:** Familiar registry patterns, event buses, and capability systems designed to make porting concepts smooth for legacy developers.
*   🧵 **Parallel Asset & Chunk Processing:** Leverages Rust's `tokio` and multithreading primitives to load custom content asynchronously.

---

## 📦 Quick Start

Ensure you have the [Rust toolchain](https://rustup.rs/) (stable) installed on your system.

### 1. Clone the Repository
```bash
git clone [https://github.com/azetechofficial/hexforge.git](https://github.com/azetechofficial/hexforge.git)
cd hexforge
