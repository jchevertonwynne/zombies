# Gauntlet is a layer on Classic, reaching it only through seams

Classic is the whole game. Gauntlet is a separate plugin added on top, whose systems run only when `Mode == Gauntlet`, and Classic code never mentions Boons, Pacts or Scrip. Gauntlet changes Classic only in three ways:

- by adding entries to the stat-modifier seam
- by reacting to messages Classic already sends (Round ended, kill, Down, Purchased)
- by reading Classic state for Requirements and Drawbacks

We chose this over a `Mode` flag checked throughout the Classic systems so that Classic stays faithful and can be tested on its own. Every Boon is designed around Classic systems, so if a Boon needs a hook Classic doesn't have, the fix is a new seam or message. It is never a Gauntlet branch inside Classic. Decided in [Spec code architecture and build order](https://github.com/jchevertonwynne/zombies/issues/35).
