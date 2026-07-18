---
packages:
  omh: patch
---

Fixed a live handoff race that could close the API connection before the success response reached the client, even though the replacement server started correctly.
