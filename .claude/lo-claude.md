# Claude Code Instructions

Project-specific instructions for Claude Code.

## Project Overview

This project uses [loclaude](https://github.com/nicholasgalante1997/loclaude) to run Claude Code with local Ollama LLMs.

## Quick Reference

```bash
# Start the LLM backend
mise run up

# Run Claude Code
mise run claude

# Check system status
mise run doctor
```

## Available Commands

| Command | Description |
|---------|-------------|
| `mise run up` | Start Ollama + Open WebUI containers |
| `mise run down` | Stop containers |
| `mise run claude` | Run Claude Code with model selection |
| `mise run models` | List installed models |
| `mise run pull <model>` | Pull a new model |
| `mise run doctor` | Check prerequisites |

## Service URLs

- **Ollama API:** http://localhost:11434
- **Open WebUI:** http://localhost:3000

## Configuration

- **Docker:** `docker-compose.yml`
- **Loclaude:** `.loclaude/config.json`
- **Tasks:** `mise.toml`

## Conventions

<!-- Add project-specific conventions here -->

## Do Not

- Commit the `models/` directory (contains large model files)
