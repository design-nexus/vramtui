# vramtui

GPU landlord for local inference. One terminal screen for the VRAM that **Ollama** and **LM Studio** share.

It shows who is on the GPU, unloads or parks a stack so the other can load, and kills orphan `llama-server` processes left behind when LM Studio crashes.

This is not a chat UI and not a bar plugin.

## Keys

| Key | Action |
| --- | --- |
| `j` / `k` | Move |
| `u` | Unload the selected model |
| `U` | Unload every model on that stack |
| `p` | Park the other stack (unload its models) |
| `l` | Load a model onto the focused stack |
| `r` | Refresh occupancy now |
| `o` | Start or stop Ollama |
| `m` | Start or stop the LM Studio server |
| `x` | Kill the selected orphan (confirm) |
| `R` | Replay the pixel wordmark |
| `?` | Help |
| `q` | Quit |

Click a tenant row to select it. Scroll the wheel to move.

The header is the design-nex.us pixel wordmark: pink → purple → indigo, drawn in then shimmering. GPU gauges, stacked VRAM, stack cards, and an inspector sit above the tenant table.

## Safety

Polling never starts LM Studio. `lms` is only run after `~/.lmstudio/.internal/http-server.json` has a live pid whose port is listening. `lms` with no daemon starts `lm-studio --run-as-service`.

Ollama is probed over HTTP (`/api/ps`). Start/stop uses systemd when the unit is loaded, otherwise a user `ollama serve` process. sudo is used only after a permission failure, with the TUI suspended so the password prompt is visible.

## Config

`~/.config/vramtui/config.toml`

```toml
ollama_host = "http://127.0.0.1:11434"
lms_path = ""          # default ~/.lmstudio/bin/lms
vram_budget_mib = 0    # 0 = total minus 2048
gpu_poll_ms = 1000
model_poll_ms = 2000
```

Theme follows the live Omarchy `colors.toml` when present (background and text), with design-nex.us pink/cyan/magenta on the chrome. Without Omarchy it uses the site palette (`#181922` / `#ff79c6` / `#8be9fd`). The wordmark gradient is always the site gradient.

## Install

```bash
./install.sh
vramtui
```
