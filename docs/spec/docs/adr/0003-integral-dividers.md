# Compartments are integral to the body

Dividers are printed as part of the body, not as a separate insert or removable divider set. This gives the strongest possible joint, no rattling, and no extra parts. The cost is that changing a layout means reprinting the body; the generator makes that cheap, and the fixed body architecture means only the division tree changes. A separate insert system was rejected because it adds height, parts, and a second fit interface.

Every divider spans its parent rectangle and is fused to the outer wall through the division tree; there are no floating dividers. See [ADR-0008](0008-recursive-division-tree.md).

**Consequences:** every body is contents-specific; the division tree is part of the spec. The body architecture must stay stable so old and new bodies still stack with each other.
