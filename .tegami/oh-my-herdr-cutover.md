---
packages:
  omh: major
  omh-docs: major
  omh-nix: major
---

### Rename the product to Oh My Herdr

This release is a breaking clean cutover to the Oh My Herdr product identity. Install and invoke `omh`, use `~/.config/omh` and `OMH_*` environment variables, and expect `omh-*` release assets from `masakirocorp/oh-my-herdr`. The old product executable, environment namespace, runtime paths, repository URLs, and release asset names are not compatibility aliases.

Herdr remains the upstream project and attribution source. Intentional Herdr compatibility names continue to use the Herdr namespace.
