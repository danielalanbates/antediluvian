# Antediluvian

A fun vertical-oriented phone game set in a prehistoric world before the great flood.

## Game Concept

**Genre:** Vertical Endless Runner / Dodger  
**Platform:** Mobile (iOS/Android)  
**Orientation:** Portrait (vertical)  
**Engine:** HTML5 + Phaser 3 (mobile-friendly)

## Gameplay

You play as a prehistoric creature trying to survive in a world of falling meteors, volcanic eruptions, and ancient predators. Collect power-ups and avoid obstacles to achieve the highest score.

### Features
- Vertical scrolling gameplay optimized for one-handed mobile play
- Prehistoric theme with dinosaurs, meteors, and ancient hazards
- Simple tap controls: tap to move left/right
- Power-ups: speed boost, shield, score multiplier
- Obstacles: falling meteors, lava pits, flying pterodactyls
- Leaderboard integration
- Responsive design for all phone sizes

## Tech Stack

- **Frontend:** HTML5, CSS3, JavaScript
- **Game Engine:** Phaser 3
- **Deployment:** GitHub Pages (alpha.batesai.org subdomain)
- **Backend:** Node.js + Express (optional, for leaderboards)

## Project Structure

```
Antediluvian/
├── index.html          # Main game page
├── css/
│   └── style.css       # Game styles
├── js/
│   ├── game.js         # Main game logic
│   ├── player.js       # Player character
│   ├── obstacles.js    # Obstacle management
│   └── powerups.js     # Power-up system
├── assets/
│   ├── sprites/        # Game sprites
│   ├── audio/          # Sound effects & music
│   └── fonts/          # Custom fonts
├── README.md           # This file
└── package.json        # Dependencies
```

## Development

### Local Development
```bash
npx http-server -p 8000
```

### Deploy to Alpha
The game deploys to: `antediluvian.batesai.org`

## License

Private - Bates LLC
