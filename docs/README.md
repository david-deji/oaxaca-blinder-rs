# Oaxaca-Blinder-RS Documentation

Welcome to the documentation for `oaxaca-blinder-rs`. This project is a high-performance Rust workspace designed for econometric decomposition, pay equity optimization, and statistical policy simulation.

## 📚 Documentation Index

- [**Architecture**](ARCHITECTURE.md): Deep dive into the system design, workspace structure, and mathematical foundations.
- [**Development Guide**](DEVELOPMENT.md): Setup instructions, testing workflows, and coding guidelines.
- [**API Reference**](API.md): Documentation for core library builders and the Meridian MCP server tools.
- [**Contributing**](../CONTRIBUTING.md): How to contribute to the project, including code review standards.

## 🎯 Purpose & Vision
The goal of this project is to provide a unified, research-grade toolkit for analyzing and narrowing group-based disparities (e.g., gender or racial pay gaps) using robust statistical methods like Oaxaca-Blinder decomposition, RIF regression, and AKM fixed effects models.

## 🛠 Technology Stack
- **Language**: Rust (2021 Edition)
- **Data Processing**: [Polars](https://pola.rs/) (DataFrame engine)
- **Linear Algebra**: [Nalgebra](https://nalgebra.org/)
- **Parallelism**: [Rayon](https://github.com/rayon-rs/rayon)
- **Interfaces**: 
    - CLI (Clap)
    - MCP Server (Axum/SSE/Stdio)
    - Python Bindings (PyO3)
    - WebAssembly (wasm-bindgen)

## 🚀 Quick Start
To run a basic decomposition via the CLI:
```bash
cargo run --bin oaxaca-cli -- report \
  --data sample.csv \
  --outcome wage \
  --group gender \
  --predictors age,education,tenure
```
