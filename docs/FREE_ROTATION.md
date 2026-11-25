# Blue Free provider selection

Kilo and JarvisClaw connect directly without API keys. On HTTP 429 the TUI offers eligible routes on the other service. Select with the arrows and Enter to send the conversation to that destination and resume the current task. Esc cancels. Terminal chat accepts a numbered selection. Unattended runs pause when selection is required. Use `--no-rotation` to disable the offer. Local Ollama stays local.

The picker sorts known model families by their published SWE-bench Verified scores, highest first. These are reference scores from model evaluations; hosted configurations can differ. Automatic routes and unscored models appear as Unranked. Reference model cards: [Nemotron Ultra](https://huggingface.co/nvidia/NVIDIA-Nemotron-3-Ultra-550B-A55B-BF16), [Nemotron Lightning](https://huggingface.co/nvidia/NVIDIA-Nemotron-3.5-Lightning-30B-A3B-BF16), [LFM3](https://huggingface.co/LiquidAI/LFM3-1.7B).

Provider cooldowns apply across local sessions. Use `/rotation` to inspect the destination list. A quota selection changes the active session; `/model` sets the startup preference.
