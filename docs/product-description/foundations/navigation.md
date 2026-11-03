# Navigation

## Summary

The right panel is a single-image wizard. Batch is a separate workspace selected
from the toolbar. The panel title names the current page.

## The simple case

Load image → mode choice → Fully automatic → Review → Export.
Skip review sends automatic completion directly to Export.

## The interaction, event by event

Basic asks for image type, optional transparency, input quality and color mode.
Fixed palette adds palette selection before fitting and Review.
Advanced goes through palette, segmentation, editing, smoothing, background and
Export. Next may read Update when processing must catch up. Back may read Revert
when changed advanced controls need to be restored before navigation.

Start returns to Load and can reset on Load. It is not the conversion command.
This label is a usability concern because it competes with Fully automatic.

## Cancel and interrupt

| Event | Before extended | While extended |
| --- | --- | --- |
| Explicit cancel | Dialog cancellation keeps the current page. | Processing cancellation prevents its queued destination from being applied. |
| Another command | Navigation follows the chosen mode. | A processing transition waits for acknowledgement. |
| Focus or process loss | Focus moves without choosing a mode. | Process loss does not preserve the navigation session. |
| Target changes outside the app | No direct page change. | File errors are reported by the affected operation. |
| Input device or modifier changes | Keyboard and pointer can navigate. | Native dialog input remains separate from wizard shortcuts. |

## Open questions and verification

Check every page can be left with both pointer and keyboard, and that controls
remain reachable at the minimum window size.

Source reviewed: `vector-magic-re` commit `e794628c8ea3fbcfd4903e55c95ac29894846715`. Runtime verification pending.
