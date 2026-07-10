import os
import random

# Procedural Data Arrays for Mobs
PREFIXES = [
    "Primordial", "Corrupted", "Star-Touched", "Feral", "Rabid", "Blood-Mad", 
    "Iron-Scaled", "Bioluminescent", "Ash-Covered", "Giant", "Dire", "Abyssal",
    "Gilded", "Venomous", "Shadow-Cursed", "Enraged", "Starving", "Mutated"
]

BASE_BEASTS = [
    "Aurochs", "Cave Bear", "Smilodon", "Mammoth", "Mastodon", "Dire-Wolf",
    "Elasmotherium", "Megatherium", "Leviathan", "Behemoth", "Serpent", 
    "Giant Eagle", "Pterosaur", "Deep-Crawler", "Slag-Salamander", "Chimera",
    "Razor-Boar", "Cave Lion", "Giant Tortoise", "Abyssal Crocodile"
]

SUFFIXES = [
    "Stalker", "Grazer", "Hunter", "Brute", "Matriarch", "Alpha", "Scavenger", 
    "Calf", "Cub", "Devourer", "Watcher-Pet", "Behemoth-Rider Mount", "Goliath"
]

REGIONS = {
    "The Edenic Basin": (1, 15),
    "The Plains of Nod": (10, 25),
    "The City of Enoch (Sewers/Outskirts)": (20, 35),
    "The Hermon Range": (30, 45),
    "The Nephilim Wastes": (40, 55),
    "The Abyssal Basins": (50, 60)
}

LOOT_TABLE = [
    "Pristine Pelt", "Tainted Meat", "Celestial Ichor", "Iron Scales", 
    "Massive Ivory Tusk", "Giant's Bone", "Glowing Venom Gland", 
    "Thick Leather", "Watcher's Core", "Abyssal Slime", "Star-Metal Dust"
]

LORE_SNIPPETS = [
    "A majestic creature of the old world, untouched by the corruption of the Watchers.",
    "Driven mad by the lack of food caused by the Nephilim expansion.",
    "It bears strange, glowing scars where celestial magic warped its DNA.",
    "Often hunted by Cainite nobles for its incredibly durable hide.",
    "Sethite nomads revere this beast and will only kill it in absolute self-defense.",
    "A horrific mutation caused by drinking water polluted by alchemical forges.",
    "It has developed a taste for human flesh after feeding on the scraps left by giants.",
    "One of the few species deemed clean enough to be loaded onto the Ark.",
    "A predator that stalks the high altitudes, ambushing Watcher cultists.",
    "A deep-earth dweller forced to the surface by the tectonic shifting of the Flood."
]

def generate_mob(mob_id):
    region, level_range = random.choice(list(REGIONS.items()))
    level = random.randint(level_range[0], level_range[1])
    
    prefix = random.choice(PREFIXES)
    base = random.choice(BASE_BEASTS)
    suffix = random.choice(SUFFIXES)
    
    # 20% chance to not have a suffix, 20% chance to not have a prefix
    has_prefix = random.random() > 0.2
    has_suffix = random.random() > 0.2
    
    name_parts = []
    if has_prefix: name_parts.append(prefix)
    name_parts.append(base)
    if has_suffix: name_parts.append(suffix)
    
    mob_name = " ".join(name_parts)
    
    temperament = random.choice(["Docile", "Territorial", "Aggressive", "Bloodthirsty", "Skittish"])
    
    is_tameable = random.random() > 0.4
    mount_type = "None"
    if is_tameable:
        if base in ["Dire-Wolf", "Smilodon", "Cave Lion"]:
            mount_type = "Swift Mount (High Speed, Low Armor)"
        elif base in ["Aurochs", "Giant Tortoise", "Megatherium"]:
            mount_type = "Pack Mount (Extra Inventory)"
        elif base in ["Cave Bear", "Elasmotherium", "Razor-Boar"]:
            mount_type = "Combat Mount (High Armor)"
        elif base in ["Mammoth", "Mastodon", "Behemoth"]:
            mount_type = "Multi-Passenger Mount (Group)"
        elif base in ["Giant Eagle", "Pterosaur"]:
            mount_type = "Gliding Mount"
        else:
            is_tameable = False
            
    loot_1 = random.choice(LOOT_TABLE)
    loot_2 = random.choice(LOOT_TABLE)
    if loot_1 == loot_2: loot_2 = "Heavy Bone"
    
    lore = random.choice(LORE_SNIPPETS)
    
    desc = f"### ID-{mob_id:04d}: {mob_name}\n"
    desc += f"**Level Range:** {level}-{level+2} | **Habitat:** {region}\n"
    desc += f"**Temperament:** {temperament} | **Tameable:** {'Yes (' + mount_type + ')' if is_tameable else 'No'}\n"
    desc += f"**Common Drops:** {loot_1}, {loot_2}\n"
    desc += f"**Lore:** {lore}\n\n"
    desc += "---\n\n"
    
    return desc

def main():
    out_dir = "/Users/daniel/Documents/Antediluvia/docs/mobs"
    os.makedirs(out_dir, exist_ok=True)
    
    total_mobs = 2500
    mobs_per_file = 100
    file_count = total_mobs // mobs_per_file
    
    mob_counter = 1
    
    for i in range(1, file_count + 1):
        filename = os.path.join(out_dir, f"Bestiary_Vol_{i:02d}.md")
        with open(filename, 'w') as f:
            f.write(f"# Antediluvia Bestiary - Volume {i}\n\n")
            f.write("This document contains procedurally generated, lore-accurate descriptions of the antediluvian fauna roaming the supercontinent.\n\n")
            
            for _ in range(mobs_per_file):
                f.write(generate_mob(mob_counter))
                mob_counter += 1
                
    print(f"Successfully generated {total_mobs} detailed animal/mob descriptions across {file_count} files in {out_dir}.")

if __name__ == "__main__":
    main()
