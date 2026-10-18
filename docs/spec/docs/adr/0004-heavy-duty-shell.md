# Heavy-duty shell with a standard-compatible lip

**Status:** superseded by [ADR-0007](0007-maximum-thickness-shell.md).

The body zone wall is 3.2 mm, the floor is 3.2 mm, and the lid is 3.2 mm, with 40% gyroid infill, for a design load of 10 kg per body and stacks of up to three bodies. The lip zone must remain 2.0 mm thick: a thicker lip closes the standard Gridfinity opening and prevents the next body's base from seating. The lip is therefore 2.0 mm, thickened below by the 3.2 mm body wall and a 45° transition; the lip carries the stack load in compression and is validated by the coupon. A uniform 3.2 mm lip was rejected because it breaks Gridfinity compatibility; a uniform 2.0 mm wall was rejected because it gives up the requested heavy-duty body.

**Consequences:** the shell is not a stock Gridfinity bin; it is a custom shell that reuses the standard base and lip profiles. The 2.0/3.2 mm step is inside the body and acts as the lid stop. The generator must not expose the wall thicknesses as free parameters.
