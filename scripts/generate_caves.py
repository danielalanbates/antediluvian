import os
import random

# Procedural Data Arrays for Caves and Mining
REGIONS = [
    "The Edenic Basin",
    "The Plains of Nod",
    "The City of Enoch",
    "The Hermon Range",
    "The Nephilim Wastes",
    "The Abyssal Basins"
]

POI_TYPES = {
    "The Edenic Basin": ["Root-Bound Cavern", "Underground Spring", "Crystal Geode Cave", "Subterranean Overgrowth", "Glimmering Hollow"],
    "The Plains of Nod": ["Dusty Mineshaft", "Sand-Choked Fissure", "Petrified Root System", "Abandoned Nomad Tunnel", "Salt Mine"],
    "The City of Enoch": ["Watcher-Tech Excavation", "Slag-Filled Cavern", "Industrial Mine", "Toxic Sump Cave", "Smuggler's Delve"],
    "The Hermon Range": ["Glacial Ice Cave", "Grigori Deep Vault", "Frozen Ore Vein", "Volcanic Vent", "Echoing Chasm"],
    "The Nephilim Wastes": ["Bone-Lined Pit", "Strip-Mined Cavern", "Fossilized Burrow", "Nephilim Hoard Cave", "Blood-Stone Mine"],
    "The Abyssal Basins": ["Flooded Magma Chamber", "Tectonic Rift", "Boiling Mud Cave", "Leviathan's Hollow", "Sulphuric Grotto"]
}

ADJECTIVES = ["ancient", "collapsing", "pristine", "abandoned", "smoldering", "frozen", "blood-soaked", "magically-irradiated", "silent", "echoing", "shadowy", "luminescent", "ruined", "fortified", "desolate", "overgrown", "crystalline", "toxic", "deep", "unfathomable"]

SIGHTS = [
    "The cave walls shimmer with an ethereal, bioluminescent fungus.",
    "Massive, crude iron chains hang from the ceiling, leading down into darkness.",
    "The rock here is unnaturally smooth, as if melted by extreme heat.",
    "Veins of glowing, unrefined ore pulse with a faint, rhythmic light.",
    "Shattered remnants of Watcher-plate mining equipment litter the floor.",
    "The cavern is completely stripped of all ore, leaving only chalky, dead rock.",
    "A soft, primeval golden light filters through a crack far above.",
    "Piles of massive, gnawed bones indicate a large predator uses this as a den.",
    "Ancient runic script is carved into the natural rock faces, glowing faintly.",
    "A subterranean river flows rapidly through the center, glowing with toxic runoff.",
    "Stalagmites and stalactites meet to form massive, natural pillars holding up the ceiling.",
    "A sheer drop leads into an apparently bottomless chasm."
]

SOUNDS = [
    "A low, subsonic hum vibrates through the soles of your feet.",
    "The distant, rhythmic pounding of pickaxes echoes endlessly from unknown miners.",
    "The wind howls through the tunnels, sounding like a chorus of weeping voices.",
    "Absolute, unnatural silence pervades the area; your own heartbeat is deafening.",
    "You can hear the frantic, desperate digging of something deep underground.",
    "The faint sound of dripping water creates a hypnotic, echoing rhythm.",
    "Heavy, wet thuds indicate the movement of something massive nearby.",
    "The crackling of chaotic, magical energy snaps the air.",
    "A continuous bubbling and hissing rises from a fissure in the floor.",
    "The guttural chants of unseen subterranean cultists drift through the tunnels."
]

HISTORY_LORE = [
    "According to Sethite tradition, this cavern was formed when the world was first spoken into existence.",
    "This was once a thriving mine before a Nephilim breached the walls and consumed the workers.",
    "The Watcher Azazel allegedly taught humanity how to extract rare metals from this very vein.",
    "Samyaza's cultists use this deep location to conduct rituals away from the light of the sun.",
    "King Lamech claimed this territory by burying a hundred rival miners alive here.",
    "This is one of the few places where the original, pre-fall magic of Eden still lingers underground.",
    "A massive, unnamed subterranean beast fell here, its fossilized ribs now forming the ceiling.",
    "Baraqiel mapped these caverns to hide Watcher-tech artifacts from humanity.",
    "Noah's emissaries hid here from the wrath of Enoch's enforcers, leaving behind small wooden totems.",
    "This area was physically warped by tectonic shifts caused by the Watchers' descent."
]

RESOURCES = [
    "Orichalcum Ore", "Celestial Copper", "Blood-Iron", "Watcher-Steel Scraps", "Luminous Quartz", 
    "Abyssal Coal", "Petrified Wood", "Nephilim Bone Fragments", "Sulfur Deposits", "Mithril-like Edenium"
]

def generate_poi(poi_id):
    region = random.choice(REGIONS)
    poi_type = random.choice(POI_TYPES[region])
    adjective = random.choice(ADJECTIVES).capitalize()
    
    name = f"CAVE-{poi_id:04d}: The {adjective} {poi_type}"
    
    sight = random.choice(SIGHTS)
    sound = random.choice(SOUNDS)
    lore = random.choice(HISTORY_LORE)
    resource = random.choice(RESOURCES)
    resource2 = random.choice(RESOURCES)
    
    # Generate a unique coordinate set (flavor), Z is depth
    coord_x = random.randint(-10000, 10000)
    coord_y = random.randint(-10000, 10000)
    coord_z = random.randint(-5000, -100)
    
    description = f"### {name}\n"
    description += f"**Region:** {region} | **Coordinates:** [{coord_x}, {coord_y}, {coord_z}]\n\n"
    description += f"**Visuals:** {sight}\n"
    description += f"**Acoustics:** {sound}\n"
    description += f"**Primary Resources:** {resource}, {resource2}\n"
    description += f"**Lore Context:** {lore}\n\n"
    description += "---\n\n"
    
    return description

def main():
    out_dir = "/Users/daniel/Documents/Antediluvia/docs/locations/caves"
    os.makedirs(out_dir, exist_ok=True)
    
    total_pois = 1000
    pois_per_file = 100
    file_count = total_pois // pois_per_file
    
    poi_counter = 1
    
    for i in range(1, file_count + 1):
        filename = os.path.join(out_dir, f"Cave_Archive_{i:02d}.md")
        with open(filename, 'w') as f:
            f.write(f"# Cave and Mine Archive Vol {i:02d}\n\n")
            f.write("A catalog of procedurally generated subterranean points of interest, mines, and caves.\n\n")
            for _ in range(pois_per_file):
                f.write(generate_poi(poi_counter))
                poi_counter += 1
                
    print(f"Successfully generated {total_pois} cave POIs across {file_count} files in {out_dir}")

if __name__ == "__main__":
    main()
