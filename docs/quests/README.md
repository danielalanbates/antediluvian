# Antediluvia Quest Database

This folder contains the complete quest designs for all five Acts of Antediluvia. These quests translate the deep mythological lore of the antediluvian period (the Book of Enoch, Genesis, the Book of Giants) into actionable MMO gameplay loops.

## Quest Structure

The game features two tiers of quests in each zone:
*   **Main Storyline Quests:** These advance the narrative of the zone. In the server, they represent the critical path (e.g., the primary kill/gather objectives).
*   **Side Quests:** Optional objectives that explore the wider lore (e.g., the corrupted teachings of the Watchers, the fallout of the Nephilim expansion).

## Index by Act

1.  [Act I: Eden](act1_eden.md) - The remnant of paradise, early strife, and the edge of the flaming sword.
2.  [Act II: Hermon](act2_hermon.md) - The high-altitude descent point of the 200 Watchers and the origins of forbidden sorcery.
3.  [Act III: Nephilim](act3_nephilim.md) - The red wasteland ravaged by the insatiable, violent giant offspring.
4.  [Act IV: Enoch](act4_enoch.md) - The massive, smog-choked first city built by Cain, fueled by Watcher metallurgy and dominated by warlords.
5.  [Act V: Flood](act5_flood.md) - The apocalyptic final days, hunting leviathans, capping geysers, and defending the Ark.

## Implementation Guidelines
Currently, `crates/server/src/world.rs` implements a single hardcoded quest per act. To implement this full database:
1.  Expand the `QUESTS` array to include all IDs listed in these documents.
2.  Update the NPC spawner to place the various `Quest Giver` entities described (e.g., Sethite Wanderer, Healer Mahalalel, Noah).
3.  Expand `CharacterSheet.quests` to handle multiple concurrent quests.
