---
packages:
  gardn: patch
---

### Keep SSH setup and remote recovery reliable

SSH setup handles login banners and large command output without corrupting results or blocking pipe transfers. Setup commands have deadlines, and disconnecting a saved connection cancels active setup. Existing SSH authentication prompts remain available.

Remote panes recover their complete retained terminal state after a long disconnection. Primary and alternate screens, input modes, and partially received terminal sequences survive reconnects even when output exceeds the worker's replay buffer.
