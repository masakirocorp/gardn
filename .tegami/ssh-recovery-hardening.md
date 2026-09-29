---
packages:
  gardn: patch
---

### Keep SSH setup and remote recovery reliable

SSH setup handles login banners and large command output without corrupting results or blocking pipe transfers. Setup commands have deadlines, and disconnecting a saved connection cancels active setup and its child processes. Unix SSH authentication retains access to the controlling terminal. Failed transfers clean up staging files even after cancellation.

Remote panes recover their complete retained terminal state after a long disconnection. Primary and alternate screens, Kitty images and placements, input modes, and partially received terminal sequences or image uploads survive reconnects even when output exceeds the worker's replay buffer. Recovery keeps the coordinator's theme, layout, and local-file restrictions. Quiet panes retry when checkpoint capacity becomes available. Failed restoration does not advance the applied output revision.
