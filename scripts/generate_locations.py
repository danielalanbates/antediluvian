import os
import random

# Procedural Data Arrays
REGIONS = [
    "The Edenic Basin",
    "The Plains of Nod",
    "The City of Enoch",
    "The Hermon Range",
    "The Nephilim Wastes",
    "The Abyssal Basins"
]

POI_TYPES = {
    "The Edenic Basin": ["Sacred Grove", "Crystal Spring", "Cherubim Burn-Scar", "First-Generation Altar", "Overgrown Monolith", "Primordial Beast Den", "Lush River Delta", "Serpent's Hollow"],
    "The Plains of Nod": ["Nomad Camp", "Dust-Swept Ruin", "Dried Oasis", "Cainite Outpost", "Caravan Wreckage", "Wind-Carved Canyon", "Lone Baobab", "Scavenger Hideout"],
    "The City of Enoch": ["Slum Ward", "Alchemical Forge", "Ziggurat Tier", "Syndicate Den", "Watcher-Tech Armory", "Smog-Choked Plaza", "Underground Catacomb", "Toxic Runoff Sewer"],
    "The Hermon Range": ["Cultist Basecamp", "Frozen Alpine Pass", "Stargazer Observatory", "Celestial Impact Crater", "Ice Cave", "Grigori Shrine", "Avalanche Zone", "Watcher's Descent Point"],
    "The Nephilim Wastes": ["Strip Mine", "Feasting Pit", "Giant's Footprint Lake", "Bone-Totem Field", "Depleted Quarry", "Blood-Stained Canyon", "Warlord's Command Tent", "Desiccated Savanna"],
    "The Abyssal Basins": ["Boiling Geyser", "Tectonic Fissure", "Flooded Marshland", "Ark Construction Yard", "Sinking Outpost", "Leviathan Shallows", "Tar Pit", "Subterranean Breach"]
}

ADJECTIVES = ["ancient", "corrupted", "pristine", "abandoned", "smoldering", "frozen", "blood-soaked", "magically-irradiated", "silent", "echoing", "shadowy", "luminescent", "ruined", "fortified", "desolate", "overgrown"]

SIGHTS = [
    "The ground is covered in strange, glowing ash.",
    "Massive, crude iron chains hang from the surrounding stone.",
    "Bioluminescent flora clings to the edges of the area.",
    "The sky above is obscured by thick, acrid smog.",
    "A strange, celestial light emanates from a crack in the earth.",
    "Shattered remnants of Watcher-plate armor litter the ground.",
    "The earth is completely stripped of all nutrients, appearing chalky and dead.",
    "A soft, primeval golden light filters through the canopy.",
    "Piles of massive, gnawed bones form a macabre perimeter.",
    "Ancient runic script is carved into the natural rock faces."
]

SOUNDS = [
    "A low, subsonic hum vibrates through the soles of your feet.",
    "The distant, rhythmic pounding of alchemical trip-hammers echoes endlessly.",
    "The wind howls through the stone, sounding like a chorus of weeping voices.",
    "Absolute, unnatural silence pervades the area; even the insects are quiet.",
    "You can hear the frantic, desperate digging of something deep underground.",
    "The faint sound of an angelic hymnal seems to play on the edge of hearing.",
    "Heavy, wet thuds indicate the movement of something massive nearby.",
    "The crackling of chaotic, magical energy snaps the air.",
    "A continuous bubbling and hissing rises from the unstable earth.",
    "The guttural chants of unseen cultists drift on the breeze."
]

HISTORY_LORE = [
    "According to Sethite tradition, this place was blessed by Enosh before the corruption spread.",
    "This was once a thriving settlement before the Nephilim consumed its inhabitants in a single night.",
    "The Watcher Azazel allegedly taught his first human disciples how to forge iron on this exact spot.",
    "Samyaza's cultists use this location to harvest toxic roots during the lunar eclipse.",
    "King Lamech claimed this territory by executing a hundred nomadic shepherds here.",
    "This is one of the few places where the original, pre-fall magic of Eden still lingers.",
    "A Nephilim warlord fell here, and the land has never recovered from his toxic blood.",
    "Baraqiel used the elevation here to map the false constellations that misguide humanity.",
    "Noah's emissaries rested here, leaving behind small wooden totems of the coming Ark.",
    "This area was physically warped when the 200 Watchers swore their oath of imprecation."
]

def generate_poi(poi_id):
    region = random.choice(REGIONS)
    poi_type = random.choice(POI_TYPES[region])
    adjective = random.choice(ADJECTIVES).capitalize()
    
    name = f"POI-{poi_id:04d}: The {adjective} {poi_type}"
    
    sight = random.choice(SIGHTS)
    sound = random.choice(SOUNDS)
    lore = random.choice(HISTORY_LORE)
    
    # Generate a unique coordinate set (flavor)
    coord_x = random.randint(-10000, 10000)
    coord_y = random.randint(-10000, 10000)
    
    description = f"### {name}\n"
    description += f"**Region:** {region} | **Coordinates:** [{coord_x}, {coord_y}]\n\n"
    description += f"**Visuals:** {sight}\n"
    description += f"**Acoustics:** {sound}\n"
    description += f"**Lore Context:** {lore}\n\n"
    description += "---\n\n"
    
    return description

def main():
    out_dir = "/Users/daniel/Documents/Antediluvia/docs/locations/points_of_interest"
    os.makedirs(out_dir, exist_ok=True)
    
    total_pois = 2000
    pois_per_file = 100
    file_count = total_pois // pois_per_file
    
    poi_counter = 1
    
    for i in range(1, file_count + 1):
        filename = os.path.join(out_dir, f"POI_Archive_{i:02d}.md")
        with open(filename, 'w') as f:
            f.write(f"# Antediluvia Points of Interest Archive - Part {i}\n\n")
            f.write("This document contains procedurally generated, lore-accurate descriptions of micro-locations across the supercontinent.\n\n")
            
            for _ in range(pois_per_file):
                f.write(generate_poi(poi_counter))
                poi_counter += 1
                
    print(f"Successfully generated {total_pois} detailed location descriptions across {file_count} files in {out_dir}.")

if __name__ == "__main__":
    main()
