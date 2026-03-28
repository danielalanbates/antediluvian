// Antediluvian - Main Game Logic
// A vertical endless runner for mobile

class MainScene extends Phaser.Scene {
    constructor() {
        super('MainScene');
    }

    create() {
        this.score = 0;
        this.gameSpeed = 200;
        this.isGameOver = false;

        // Create background gradient
        this.createBackground();

        // Player setup
        this.createPlayer();

        // Input handling
        this.setupControls();

        // Groups
        this.obstacles = this.physics.add.group();
        this.powerups = this.physics.add.group();

        // Spawning timers
        this.time.addEvent({
            delay: 1500,
            callback: this.spawnObstacle,
            callbackScope: this,
            loop: true
        });

        this.time.addEvent({
            delay: 3000,
            callback: this.spawnPowerup,
            callbackScope: this,
            loop: true
        });

        // Score timer
        this.time.addEvent({
            delay: 100,
            callback: () => {
                if (!this.isGameOver) {
                    this.score += 1;
                    this.updateScore();
                    // Increase difficulty
                    if (this.score % 500 === 0) {
                        this.gameSpeed += 20;
                    }
                }
            },
            loop: true
        });

        // Collisions
        this.physics.add.overlap(this.player, this.obstacles, this.hitObstacle, null, this);
        this.physics.add.overlap(this.player, this.powerups, this.collectPowerup, null, this);
    }

    createBackground() {
        // Create animated background with prehistoric theme
        const graphics = this.add.graphics();
        
        // Sky gradient
        const sky = this.add.rectangle(0, 0, this.scale.width, this.scale.height, 0x1a0a2e);
        sky.setOrigin(0);

        // Distant volcanoes
        this.createVolcanoes();

        // Falling meteors in background
        this.meteors = this.add.group();
        this.time.addEvent({
            delay: 2000,
            callback: this.spawnBackgroundMeteor,
            callbackScope: this,
            loop: true
        });
    }

    createVolcanoes() {
        const graphics = this.add.graphics();
        graphics.fillStyle(0x2a0a3e, 1);
        
        // Draw volcano silhouettes
        for (let i = 0; i < 5; i++) {
            const x = Phaser.Math.Between(0, this.scale.width);
            const y = this.scale.height - 50;
            const width = Phaser.Math.Between(80, 150);
            const height = Phaser.Math.Between(100, 200);
            
            graphics.beginPath();
            graphics.moveTo(x - width/2, y);
            graphics.lineTo(x, y - height);
            graphics.lineTo(x + width/2, y);
            graphics.closePath();
            graphics.fillPath();
        }
    }

    createPlayer() {
        // Player is a prehistoric creature (simple shape for now)
        const width = Math.min(60, this.scale.width * 0.15);
        const height = width;

        this.player = this.physics.add.circle(
            this.scale.width / 2,
            this.scale.height - 150,
            width / 2,
            0x00ff88
        );
        this.player.setCollideWorldBounds(true);
        this.player.setDrag(500);
        this.player.setMaxVelocity(400);

        // Add glow effect
        const playerGlow = this.add.circle(
            this.player.x,
            this.player.y,
            width / 2 + 5,
            0x00ff88,
            0.3
        );
        playerGlow.setStrokeStyle(2, 0x00ff88);
        
        // Store reference for updates
        this.playerGlow = playerGlow;
    }

    setupControls() {
        // Touch controls for mobile
        this.input.on('pointermove', (pointer) => {
            if (this.player && !this.isGameOver) {
                const targetX = pointer.x;
                this.physics.moveTo(this.player, targetX, this.player.y, 500);
            }
        });

        // Keyboard controls for testing
        this.cursors = this.input.keyboard.createCursorKeys();
    }

    update() {
        if (this.isGameOver) return;

        // Keyboard controls
        if (this.cursors.left.isDown) {
            this.player.setVelocityX(-300);
        } else if (this.cursors.right.isDown) {
            this.player.setVelocityX(300);
        }

        // Update player glow position
        if (this.playerGlow) {
            this.playerGlow.x = this.player.x;
            this.playerGlow.y = this.player.y;
        }

        // Update background meteors
        if (this.meteors) {
            this.meteors.getChildren().forEach(meteor => {
                meteor.y += 3;
                if (meteor.y > this.scale.height) {
                    meteor.destroy();
                }
            });
        }
    }

    spawnObstacle() {
        if (this.isGameOver) return;

        const types = ['meteor', 'lava', 'pterodactyl'];
        const type = Phaser.Math.RND.pick(types);
        const x = Phaser.Math.Between(50, this.scale.width - 50);

        let obstacle;
        
        switch(type) {
            case 'meteor':
                obstacle = this.physics.add.circle(x, -50, 25, 0xff4400);
                break;
            case 'lava':
                obstacle = this.physics.add.circle(x, -50, 35, 0xff6600);
                break;
            case 'pterodactyl':
                obstacle = this.physics.add.rectangle(x, -50, 50, 30, 0x8844ff);
                break;
        }

        if (obstacle) {
            obstacle.setData('type', type);
            this.obstacles.add(obstacle);
        }
    }

    spawnPowerup() {
        if (this.isGameOver) return;

        const x = Phaser.Math.Between(50, this.scale.width - 50);
        const types = [
            { color: 0xffff00, name: 'star' },
            { color: 0x0088ff, name: 'shield' },
            { color: 0xff0088, name: 'multiplier' }
        ];
        const powerupType = Phaser.Math.RND.pick(types);

        const powerup = this.physics.add.circle(x, -50, 20, powerupType.color);
        powerup.setData('name', powerupType.name);
        this.powerups.add(powerup);
    }

    spawnBackgroundMeteor() {
        const x = Phaser.Math.Between(0, this.scale.width);
        const meteor = this.add.circle(x, -10, 3, 0xff6633, 0.6);
        this.meteors.add(meteor);
    }

    hitObstacle(player, obstacle) {
        if (this.isGameOver) return;

        this.isGameOver = true;
        this.physics.pause();
        
        // Explosion effect
        const explosion = this.add.circle(player.x, player.y, 50, 0xff4400, 0.8);
        this.tweens.add({
            targets: explosion,
            scale: 2,
            alpha: 0,
            duration: 500
        });

        // Show game over screen
        setTimeout(() => {
            document.getElementById('final-score').textContent = `Score: ${this.score}`;
            document.getElementById('game-over').style.display = 'flex';
        }, 500);
    }

    collectPowerup(player, powerup) {
        const powerupName = powerup.getData('name');
        
        switch(powerupName) {
            case 'star':
                this.score += 100;
                break;
            case 'shield':
                // Temporary invincibility could be added here
                this.score += 50;
                break;
            case 'multiplier':
                this.score += 75;
                break;
        }

        this.updateScore();
        powerup.destroy();
    }

    updateScore() {
        document.getElementById('score').textContent = this.score;
    }

    restart() {
        this.scene.restart();
    }
}

// Game configuration
const config = {
    type: Phaser.AUTO,
    parent: 'game-container',
    width: window.innerWidth,
    height: window.innerHeight,
    transparent: false,
    physics: {
        default: 'arcade',
        arcade: {
            gravity: { y: 0 },
            debug: false
        }
    },
    scene: MainScene,
    scale: {
        mode: Phaser.Scale.RESIZE,
        autoCenter: Phaser.Scale.CENTER_BOTH
    }
};

// Initialize game
let game;

function startGame() {
    document.getElementById('start-screen').style.display = 'none';
    document.getElementById('score-display').style.display = 'block';
    
    if (!game) {
        game = new Phaser.Game(config);
    }
}

function restartGame() {
    document.getElementById('game-over').style.display = 'none';
    document.getElementById('score').textContent = '0';
    
    if (game) {
        game.scene.getScene('MainScene').restart();
    }
}

// Event listeners
document.getElementById('start-btn').addEventListener('click', startGame);
document.getElementById('restart-btn').addEventListener('click', restartGame);

// Handle window resize
window.addEventListener('resize', () => {
    if (game) {
        game.scale.resize(window.innerWidth, window.innerHeight);
    }
});
