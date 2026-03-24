# Contributing to StellarLend 🚀

Thanks for your interest in contributing.

This guide explains how to set up the project and submit changes.

---

## 1. Project Structure

```
stellarlend/
├── api/              # backend (Node.js + TypeScript)
├── oracle/           # oracle service
├── landing/          # frontend (static)
├── stellar-lend/     # core protocol (Rust / Soroban)
├── docs/             # documentation
├── scripts/          # helper scripts
├── *.md              # project notes and summaries
├── vercel.json       # deployment config
└── ...
```

---

## 2. Prerequisites

Make sure you have the following installed:

* Node.js (v18+)
* npm
* Rust (latest stable)
* Soroban CLI
* Git

### Install Rust

```bash
curl https://sh.rustup.rs -sSf | sh
```

### Install Soroban CLI

```bash
cargo install soroban-cli
```

---

## 3. Local Setup

### Clone the repository

```bash
git clone https://github.com/Smartdevs17/stellarlend.git
cd stellarlend
```

---

### API Setup

```bash
cd api
npm install
```

Create a `.env` file:

```bash
cp .env.example .env  
```

If not:

```bash
touch .env
```

Minimum required variables:

```env
CONTRACT_ID=dummy_contract_id
JWT_SECRET=dev_secret_key_min_32_chars_long
```

Start the server:

```bash
npm run dev
```

---

### Oracle Setup

```bash
cd oracle
npm install
npm run dev
```

---

### Contracts (Soroban / Rust)

```bash
cd stellar-lend
cargo build
```

---

## 4. Branching

Use clear and descriptive branch names:

* `fix/...`
* `feat/...`
* `docs/...`

Example:

```bash
git checkout -b fix/issue-name
```

---

## 5. Commit Messages

Use the format:

```text
type(scope): short description
```

Examples:

* `fix(api): handle invalid input`
* `docs: update contributing guide`

---

## 6. Pull Request Process

1. Fork the repository
2. Create a new branch
3. Make your changes
4. Ensure the project runs locally and tests pass
5. Commit and push
6. Open a Pull Request

### Requirements

* Link the issue:

```text
Closes #123
```

* Keep changes small and focused
* Avoid unrelated changes

---

## 7. Testing

Run tests before submitting:

### API

```bash
cd api
npm test
```

### Oracle

```bash
cd oracle
npm test
```

### Contracts

```bash
cd stellar-lend
cargo test
```

---

## 8. Guidelines

* Do not commit `.env` files
* Do not expose secrets in logs or responses
* Keep code simple and readable
* Prefer small, focused changes

---

## 9. Additional Resources

* Stellar Docs: https://developers.stellar.org/
* Soroban Docs: https://soroban.stellar.org/

---


