---
status: accepted
---

# Model group default agent profiles as favorites

Gardn stores a workspace group's favorite agent profile ids and default agent profile id separately, but mutation paths enforce that the default profile is also a favorite. Setting a default adds the profile to favorites if needed, removing a favorite clears it as the default, and New Agent first tries the target workspace group's launchable default before opening the picker or falling back to the singleton-profile fast path.

This makes default launch behavior visible in the same group-scoped New Agent surface users use to organize profiles. A hidden default outside favorites would be more flexible internally, but it would let a group auto-launch a profile that is not promoted in the picker or settings UI; storing the fields separately still lets Gardn distinguish the default from the rest of the favorite ordering.

This is separate from ADR 0016's agent profile catalog decision. ADR 0016 records global profile identity, profile ordering, and integration authority boundaries; this ADR records the per-group relationship between favorites, defaults, and the New Agent launch fast path.

## Current rationale

`[INFERENCE]` Gardn treats defaults as favorites so the fastest launch path remains explainable: the profile a group auto-launches is also one of the profiles the group visibly promotes. The direct launch path reduces friction for common single/default-agent workflows without creating a second hidden preference model.

## Consequences

New settings or API behavior that changes a group default must preserve the invariant that the default is launchable and favorited. New New Agent entry points should resolve defaults from the target workspace's group, not the currently active group when those differ.
