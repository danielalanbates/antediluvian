import Phaser from 'phaser'

interface GameData {
  act: string
}

export class GameScene extends Phaser.Scene {
  private player!: Phaser.Physics.Arcade.Sprite
  private sword!: Phaser.GameObjects.Sprite
  private cursors!: Phaser.Types.Input.Keyboard.CursorKeys
  private keys!: any
  private currentAct: string = 'eden'
  private npcs!: Phaser.Physics.Arcade.Group
  private enemies!: Phaser.Physics.Arcade.Group
  private wildlife!: Phaser.Physics.Arcade.Group
  private resources!: Phaser.Physics.Arcade.Group
  private projectiles!: Phaser.Physics.Arcade.Group
  private interactPrompt!: Phaser.GameObjects.Text
  private isAttacking: boolean = false
  private playerHealth: number = 400
  private playerMaxHealth: number = 200
  private lightingOverlay!: Phaser.GameObjects.Graphics
  private bloomGraphics!: Phaser.GameObjects.Graphics
  private vignetteGraphics!: Phaser.GameObjects.Graphics
  private weatherParticles!: Phaser.GameObjects.Particles.ParticleEmitter | null
  private actLabel!: Phaser.GameObjects.Text
  private lastDirection: string = 'down'
  private attackHitbox!: Phaser.GameObjects.Zone
  private bloomTime: number = 0

  constructor() {
    super({ key: 'GameScene' })
  }

  create(data: GameData) {
    console.log('GameScene: Creating game with act:', data.act)

    this.currentAct = data.act
    const width = this.cameras.main.width
    const height = this.cameras.main.height

    // Create the game world
    this.createWorld()

    // Create player
    this.player = this.physics.add.sprite(width / 4, height / 4, 'player')
    this.player.setCollideWorldBounds(true)
    this.player.setDepth(10)
    this.player.setDisplaySize(256, 320)
    
    // Create sword (initially hidden, shown during attacks)
    this.sword = this.add.sprite(0, 0, 'sword_sheathed')
      .setDepth(15)
      .setVisible(true)
      .setAlpha(0.9)
      .setDisplaySize(48, 160)

    // Create groups
    this.npcs = this.physics.add.group()
    this.enemies = this.physics.add.group()
    this.wildlife = this.physics.add.group()
    this.resources = this.physics.add.group()
    this.projectiles = this.physics.add.group()

    // Attack hitbox (melee range) — Zone is invisible by design, used only for overlap detection
    this.attackHitbox = this.add.zone(0, 0, 120, 120)
      .setDepth(-1000)
      .setActive(true)

    // Create world entities based on act
    this.createActEntities()

    // Setup controls
    this.cursors = this.input.keyboard!.createCursorKeys()
    this.keys = this.input.keyboard!.addKeys({
      up: Phaser.Input.Keyboard.KeyCodes.W,
      down: Phaser.Input.Keyboard.KeyCodes.S,
      left: Phaser.Input.Keyboard.KeyCodes.A,
      right: Phaser.Input.Keyboard.KeyCodes.D,
      attack: Phaser.Input.Keyboard.KeyCodes.SPACE,
      interact: Phaser.Input.Keyboard.KeyCodes.E,
      act1: Phaser.Input.Keyboard.KeyCodes.ONE,
      act2: Phaser.Input.Keyboard.KeyCodes.TWO,
      act3: Phaser.Input.Keyboard.KeyCodes.THREE,
      act4: Phaser.Input.Keyboard.KeyCodes.FOUR,
      act5: Phaser.Input.Keyboard.KeyCodes.FIVE
    })

    // Collisions
    this.physics.add.collider(this.player, this.npcs)
    this.physics.add.collider(this.player, this.resources)
    this.physics.add.collider(this.player, this.wildlife)
    this.physics.add.overlap(this.attackHitbox, this.enemies, this.meleeHitEnemy, undefined, this)
    this.physics.add.overlap(this.projectiles, this.enemies, this.hitEnemy, undefined, this)
    this.physics.add.overlap(this.player, this.enemies, this.playerHitEnemy, undefined, this)

    // Interaction prompt
    this.interactPrompt = this.add.text(width / 4, height - 400, '', {
      font: '58px Arial',
      color: '#ffffff',
      stroke: '#000000',
      strokeThickness: 3
    }).setOrigin(0.5).setVisible(false).setDepth(1000)

    // Camera follows player
    this.cameras.main.startFollow(this.player, true, 0.08, 0.08)
    this.cameras.main.setZoom(0.30)
    this.cameras.main.setBackgroundColor('#1a2a1a')

    // Lighting overlay
    this.lightingOverlay = this.add.graphics().setDepth(999)
    this.lightingOverlay.setBlendMode(Phaser.BlendModes.MULTIPLY)

    // Bloom overlay - creates a glow effect on bright elements (HD-2D style)
    this.bloomGraphics = this.add.graphics().setDepth(998)
    this.bloomGraphics.setBlendMode(Phaser.BlendModes.ADD)

    // Vignette overlay - darker edges for cinematic feel
    this.vignetteGraphics = this.add.graphics().setDepth(1000)
    this.vignetteGraphics.setBlendMode(Phaser.BlendModes.MULTIPLY)

    // Act label (fades out)
    this.actLabel = this.add.text(width / 2, height / 3, '', {
      font: 'bold 65px Arial',
      color: '#ffd700',
      stroke: '#000000',
      strokeThickness: 6
    }).setOrigin(0.5).setDepth(1001).setAlpha(0)

    // Weather effects per act
    this.createWeather()

    // Show act name briefly
    this.showActName()

    console.log('GameScene: Game created successfully!')
  }

  createWorld() {
    const worldWidth = 6000
    const worldHeight = 6000

    // Create ground with tile texture
    const ground = this.add.tileSprite(worldWidth / 4, worldHeight / 4, worldWidth, worldHeight, 'grass')
    ground.setDepth(-2)

    // Background layer (slightly offset for parallax)
    const bgLayer = this.add.tileSprite(worldWidth / 4, worldHeight / 4, worldWidth, worldHeight, 'dirt')
    bgLayer.setDepth(-3)
    bgLayer.setAlpha(0.3)

    // Set world bounds
    this.physics.world.setBounds(0, 0, worldWidth, worldHeight)
  }

  createWeather() {
    // Clear existing
    if (this.weatherParticles) {
      this.weatherParticles.destroy()
      this.weatherParticles = null
    }

    switch (this.currentAct) {
      case 'flood':
        // Rain particles
        this.weatherParticles = this.add.particles(0, 0, 'exp_particle', {
          x: { min: 0, max: 3000 },
          y: -20,
          speedY: { min: 600, max: 1000 },
          speedX: { min: -100, max: 100 },
          lifespan: 3000,
          frequency: 68,
          scale: { start: 1.2, end: 0 },
          alpha: { start: 0.4, end: 0 },
          tint: 0x88bbdd,
          emitting: true
        }).setDepth(998)
        break
      case 'nephilim':
        // Ash/ember particles
        this.weatherParticles = this.add.particles(0, 0, 'exp_particle', {
          x: { min: 0, max: 3000 },
          y: { min: 0, max: 3000 },
          speedY: { min: -40, max: 40 },
          speedX: { min: -60, max: 60 },
          lifespan: 4000,
          frequency: 225,
          scale: { start: 2.0, end: 0 },
          alpha: { start: 0.5, end: 0 },
          tint: [0xff4400, 0xff8800, 0xffaa00],
          emitting: true
        }).setDepth(998)
        break
      case 'enoch':
        // Divine sparkles
        this.weatherParticles = this.add.particles(0, 0, 'exp_particle', {
          x: { min: 0, max: 3000 },
          y: { min: 0, max: 3000 },
          speedY: { min: -20, max: 20 },
          speedX: { min: -20, max: 20 },
          lifespan: 3000,
          frequency: 450,
          scale: { start: 1.6, end: 0 },
          alpha: { start: 0.6, end: 0 },
          tint: 0xffd700,
          emitting: true
        }).setDepth(998)
        break
    }
  }

  showActName() {
    const actNames: Record<string, string> = {
      eden: '🌿 The Garden of Eden',
      hermon: '⛰️ Mount Hermon',
      nephilim: '👹 Nephilim Territories',
      enoch: '✨ Enoch\'s Journey',
      flood: '🌊 The Great Flood'
    }

    const name = actNames[this.currentAct] || this.currentAct
    this.actLabel.setText(name)
    this.actLabel.setAlpha(1)

    this.tweens.add({
      targets: this.actLabel,
      alpha: 0,
      duration: 2000,
      delay: 1500,
      ease: 'Power2'
    })
  }

  createActEntities() {
    switch (this.currentAct) {
      case 'eden':
        this.createEden()
        break
      case 'hermon':
        this.createMountHermon()
        break
      case 'nephilim':
        this.createNephilimTerritories()
        break
      case 'enoch':
        this.createEnochJourney()
        break
      case 'flood':
        this.createTheFlood()
        break
    }
  }

  createEden() {
    console.log('GameScene: Creating Eden...')

    // Lush trees
    for (let i = 0; i < 25; i++) {
      const x = Phaser.Math.Between(400, 11600)
      const y = Phaser.Math.Between(400, 11600)
      const tree = this.resources.create(x, y, 'tree')
      tree.setImmovable(true)
      tree.setDisplaySize(128, 160)
      tree.setDepth(5)
    }

    // Herbs scattered
    for (let i = 0; i < 30; i++) {
      const x = Phaser.Math.Between(400, 11600)
      const y = Phaser.Math.Between(400, 11600)
      const herb = this.resources.create(x, y, 'herb')
      herb.setDepth(3)
    }

    // Rocks
    for (let i = 0; i < 15; i++) {
      const x = Phaser.Math.Between(400, 11600)
      const y = Phaser.Math.Between(400, 11600)
      const rock = this.resources.create(x, y, 'rock')
      rock.setImmovable(true)
      rock.setDisplaySize(96, 80)
      rock.setDepth(4)
    }

    // NPCs
    const adam = this.npcs.create(800, 900, 'npc_elder')
    adam.setDisplaySize(96, 128)
    adam.setDepth(8)
    adam.setData('name', 'Adam')
    adam.setData('dialogue', 'Welcome to Eden, traveler. The garden is bountiful. Tend to it well.')

    const eve = this.npcs.create(900, 900, 'npc_woman')
    eve.setDisplaySize(96, 128)
    eve.setDepth(8)
    eve.setData('name', 'Eve')
    eve.setData('dialogue', 'The herbs here have healing properties. Gather them wisely.')

    // Chest
    const chest = this.resources.create(1200, 800, 'chest')
    chest.setDepth(6)
    chest.setData('type', 'chest')
    chest.setData('loot', 'Health Herb x3')

    // Passive wildlife - deer
    for (let i = 0; i < 12; i++) {
      const x = Phaser.Math.Between(800, 11200)
      const y = Phaser.Math.Between(800, 11200)
      const deer = this.wildlife.create(x, y, 'deer')
      deer.setDisplaySize(144, 120)
      deer.setDepth(6)
      deer.setData('type', 'deer')
      deer.setData('fleeRange', 480)
      deer.setData('patrolRadius', 320)
      deer.setData('originX', x)
      deer.setData('originY', y)
      deer.setData('state', 'graze')
      deer.setData('stateTimer', 0)
      deer.setData('speed', 160)
    }

    // Passive wildlife - rabbits
    for (let i = 0; i < 15; i++) {
      const x = Phaser.Math.Between(800, 11200)
      const y = Phaser.Math.Between(800, 11200)
      const rabbit = this.wildlife.create(x, y, 'rabbit')
      rabbit.setDisplaySize(96, 84)
      rabbit.setDepth(6)
      rabbit.setData('type', 'rabbit')
      rabbit.setData('fleeRange', 400)
      rabbit.setData('patrolRadius', 240)
      rabbit.setData('originX', x)
      rabbit.setData('originY', y)
      rabbit.setData('state', 'graze')
      rabbit.setData('stateTimer', 0)
      rabbit.setData('speed', 200)
    }

    // Passive wildlife - birds
    for (let i = 0; i < 10; i++) {
      const x = Phaser.Math.Between(800, 11200)
      const y = Phaser.Math.Between(800, 11200)
      const bird = this.wildlife.create(x, y, 'bird')
      bird.setDisplaySize(84, 72)
      bird.setDepth(6)
      bird.setData('type', 'bird')
      bird.setData('fleeRange', 320)
      bird.setData('patrolRadius', 400)
      bird.setData('originX', x)
      bird.setData('originY', y)
      bird.setData('state', 'graze')
      bird.setData('stateTimer', 0)
      bird.setData('speed', 240)
    }

    // Passive wildlife - foxes
    for (let i = 0; i < 6; i++) {
      const x = Phaser.Math.Between(800, 11200)
      const y = Phaser.Math.Between(800, 11200)
      const fox = this.wildlife.create(x, y, 'fox')
      fox.setDisplaySize(120, 96)
      fox.setDepth(6)
      fox.setData('type', 'fox')
      fox.setData('fleeRange', 560)
      fox.setData('patrolRadius', 360)
      fox.setData('originX', x)
      fox.setData('originY', y)
      fox.setData('state', 'graze')
      fox.setData('stateTimer', 0)
      fox.setData('speed', 220)
    }

    // Aggressive mobs - serpents
    for (let i = 0; i < 10; i++) {
      const x = Phaser.Math.Between(800, 11200)
      const y = Phaser.Math.Between(800, 11200)
      const enemy = this.enemies.create(x, y, 'serpent_mob')
      enemy.setDisplaySize(144, 84)
      enemy.setDepth(7)
      enemy.setData('health', 200)
      enemy.setData('maxHealth', 200)
      enemy.setData('damage', 40)
      enemy.setData('type', 'serpent')
      enemy.setData('aggroRange', 600)
      enemy.setData('patrolRadius', 400)
      enemy.setData('originX', x)
      enemy.setData('originY', y)
      enemy.setData('state', 'patrol')
      enemy.setData('stateTimer', 0)
      enemy.setData('attackCooldown', 0)
      enemy.setData('speed', 240)
    }
  }

  createMountHermon() {
    console.log('GameScene: Creating Mount Hermon...')

    // Rocky terrain
    for (let i = 0; i < 40; i++) {
      const x = Phaser.Math.Between(400, 11600)
      const y = Phaser.Math.Between(400, 11600)
      const rock = this.resources.create(x, y, 'rock')
      rock.setImmovable(true)
      rock.setDisplaySize(96, 80)
      rock.setDepth(4)
    }

    // Sparse trees
    for (let i = 0; i < 10; i++) {
      const x = Phaser.Math.Between(400, 11600)
      const y = Phaser.Math.Between(400, 11600)
      const tree = this.resources.create(x, y, 'tree')
      tree.setImmovable(true)
      tree.setDisplaySize(128, 160)
      tree.setDepth(5)
    }

    // Watcher angel
    const watcher = this.npcs.create(1500, 1500, 'npc_elder')
    watcher.setDisplaySize(132, 164)
    watcher.setDepth(8)
    watcher.setData('name', 'Watcher Angel')
    watcher.setData('dialogue', 'We descended to teach mankind the ways of the heavens...')

    // Passive wildlife - goats
    for (let i = 0; i < 10; i++) {
      const x = Phaser.Math.Between(800, 11200)
      const y = Phaser.Math.Between(800, 11200)
      const goat = this.wildlife.create(x, y, 'goat')
      goat.setDisplaySize(120, 96)
      goat.setDepth(6)
      goat.setData('type', 'goat')
      goat.setData('fleeRange', 520)
      goat.setData('patrolRadius', 280)
      goat.setData('originX', x)
      goat.setData('originY', y)
      goat.setData('state', 'graze')
      goat.setData('stateTimer', 0)
      goat.setData('speed', 180)
    }

    // Passive wildlife - sheep
    for (let i = 0; i < 8; i++) {
      const x = Phaser.Math.Between(800, 11200)
      const y = Phaser.Math.Between(800, 11200)
      const sheep = this.wildlife.create(x, y, 'sheep')
      sheep.setDisplaySize(120, 96)
      sheep.setDepth(6)
      sheep.setData('type', 'sheep')
      sheep.setData('fleeRange', 440)
      sheep.setData('patrolRadius', 240)
      sheep.setData('originX', x)
      sheep.setData('originY', y)
      sheep.setData('state', 'graze')
      sheep.setData('stateTimer', 0)
      sheep.setData('speed', 140)
    }

    // Passive wildlife - foxes
    for (let i = 0; i < 5; i++) {
      const x = Phaser.Math.Between(800, 11200)
      const y = Phaser.Math.Between(800, 11200)
      const fox = this.wildlife.create(x, y, 'fox')
      fox.setDisplaySize(120, 96)
      fox.setDepth(6)
      fox.setData('type', 'fox')
      fox.setData('fleeRange', 560)
      fox.setData('patrolRadius', 360)
      fox.setData('originX', x)
      fox.setData('originY', y)
      fox.setData('state', 'graze')
      fox.setData('stateTimer', 0)
      fox.setData('speed', 220)
    }

    // Aggressive mobs - watchers (fallen)
    for (let i = 0; i < 12; i++) {
      const x = Phaser.Math.Between(800, 11200)
      const y = Phaser.Math.Between(800, 11200)
      const enemy = this.enemies.create(x, y, 'watcher_mob')
      enemy.setDisplaySize(132, 164)
      enemy.setDepth(7)
      enemy.setData('health', 240)
      enemy.setData('maxHealth', 240)
      enemy.setData('damage', 60)
      enemy.setData('type', 'watcher')
      enemy.setData('aggroRange', 720)
      enemy.setData('patrolRadius', 480)
      enemy.setData('originX', x)
      enemy.setData('originY', y)
      enemy.setData('state', 'patrol')
      enemy.setData('stateTimer', 0)
      enemy.setData('attackCooldown', 0)
      enemy.setData('speed', 200)
    }

    // Aggressive mobs - Seth warriors
    for (let i = 0; i < 8; i++) {
      const x = Phaser.Math.Between(800, 11200)
      const y = Phaser.Math.Between(800, 11200)
      const enemy = this.enemies.create(x, y, 'seth_warrior')
      enemy.setDisplaySize(120, 164)
      enemy.setDepth(7)
      enemy.setData('health', 320)
      enemy.setData('maxHealth', 320)
      enemy.setData('damage', 80)
      enemy.setData('type', 'seth_warrior')
      enemy.setData('aggroRange', 640)
      enemy.setData('patrolRadius', 400)
      enemy.setData('originX', x)
      enemy.setData('originY', y)
      enemy.setData('state', 'patrol')
      enemy.setData('stateTimer', 0)
      enemy.setData('attackCooldown', 0)
      enemy.setData('speed', 180)
    }
  }

  createNephilimTerritories() {
    console.log('GameScene: Creating Nephilim Territories...')

    // Destroyed structures
    for (let i = 0; i < 15; i++) {
      const x = Phaser.Math.Between(400, 11600)
      const y = Phaser.Math.Between(400, 11600)
      const house = this.resources.create(x, y, 'house')
      house.setImmovable(true)
      house.setDisplaySize(160, 140)
      house.setDepth(5)
    }

    // Rocks everywhere
    for (let i = 0; i < 30; i++) {
      const x = Phaser.Math.Between(400, 11600)
      const y = Phaser.Math.Between(400, 11600)
      const rock = this.resources.create(x, y, 'rock')
      rock.setImmovable(true)
      rock.setDisplaySize(96, 80)
      rock.setDepth(4)
    }

    // Noah
    const noah = this.npcs.create(1500, 1500, 'npc_elder')
    noah.setDisplaySize(96, 128)
    noah.setDepth(8)
    noah.setData('name', 'Noah')
    noah.setData('dialogue', 'Build an ark... the flood is coming. God has spoken to me.')

    // Few passive wildlife - scared rabbits
    for (let i = 0; i < 4; i++) {
      const x = Phaser.Math.Between(800, 11200)
      const y = Phaser.Math.Between(800, 11200)
      const rabbit = this.wildlife.create(x, y, 'rabbit')
      rabbit.setDisplaySize(96, 84)
      rabbit.setDepth(6)
      rabbit.setData('type', 'rabbit')
      rabbit.setData('fleeRange', 600)
      rabbit.setData('patrolRadius', 160)
      rabbit.setData('originX', x)
      rabbit.setData('originY', y)
      rabbit.setData('state', 'graze')
      rabbit.setData('stateTimer', 0)
      rabbit.setData('speed', 220)
    }

    // Nephilim giants (strong enemies)
    for (let i = 0; i < 12; i++) {
      const x = Phaser.Math.Between(800, 11200)
      const y = Phaser.Math.Between(800, 11200)
      const enemy = this.enemies.create(x, y, 'nephilim_giant')
      enemy.setDisplaySize(150, 194)
      enemy.setDepth(7)
      enemy.setData('health', 480)
      enemy.setData('maxHealth', 480)
      enemy.setData('damage', 100)
      enemy.setData('type', 'nephilim')
      enemy.setData('aggroRange', 800)
      enemy.setData('patrolRadius', 320)
      enemy.setData('originX', x)
      enemy.setData('originY', y)
      enemy.setData('state', 'patrol')
      enemy.setData('stateTimer', 0)
      enemy.setData('attackCooldown', 0)
      enemy.setData('speed', 140)
    }

    // Demons
    for (let i = 0; i < 10; i++) {
      const x = Phaser.Math.Between(800, 11200)
      const y = Phaser.Math.Between(800, 11200)
      const enemy = this.enemies.create(x, y, 'demon_mob')
      enemy.setDisplaySize(132, 150)
      enemy.setDepth(7)
      enemy.setData('health', 320)
      enemy.setData('maxHealth', 320)
      enemy.setData('damage', 80)
      enemy.setData('type', 'demon')
      enemy.setData('aggroRange', 640)
      enemy.setData('patrolRadius', 400)
      enemy.setData('originX', x)
      enemy.setData('originY', y)
      enemy.setData('state', 'patrol')
      enemy.setData('stateTimer', 0)
      enemy.setData('attackCooldown', 0)
      enemy.setData('speed', 220)
    }

    // Seth warriors
    for (let i = 0; i < 8; i++) {
      const x = Phaser.Math.Between(800, 11200)
      const y = Phaser.Math.Between(800, 11200)
      const enemy = this.enemies.create(x, y, 'seth_warrior')
      enemy.setDisplaySize(120, 164)
      enemy.setDepth(7)
      enemy.setData('health', 320)
      enemy.setData('maxHealth', 320)
      enemy.setData('damage', 80)
      enemy.setData('type', 'seth_warrior')
      enemy.setData('aggroRange', 640)
      enemy.setData('patrolRadius', 400)
      enemy.setData('originX', x)
      enemy.setData('originY', y)
      enemy.setData('state', 'patrol')
      enemy.setData('stateTimer', 0)
      enemy.setData('attackCooldown', 0)
      enemy.setData('speed', 180)
    }
  }

  createEnochJourney() {
    console.log('GameScene: Creating Enoch\'s Journey...')

    // Mixed terrain
    for (let i = 0; i < 20; i++) {
      const x = Phaser.Math.Between(400, 11600)
      const y = Phaser.Math.Between(400, 11600)
      const tree = this.resources.create(x, y, 'tree')
      tree.setImmovable(true)
      tree.setDisplaySize(128, 160)
      tree.setDepth(5)
    }

    for (let i = 0; i < 25; i++) {
      const x = Phaser.Math.Between(400, 11600)
      const y = Phaser.Math.Between(400, 11600)
      const rock = this.resources.create(x, y, 'rock')
      rock.setImmovable(true)
      rock.setDisplaySize(96, 80)
      rock.setDepth(4)
    }

    // Herbs
    for (let i = 0; i < 15; i++) {
      const x = Phaser.Math.Between(400, 11600)
      const y = Phaser.Math.Between(400, 11600)
      const herb = this.resources.create(x, y, 'herb')
      herb.setDepth(3)
    }

    // Enoch
    const enoch = this.npcs.create(1500, 1500, 'npc_elder')
    enoch.setDisplaySize(96, 128)
    enoch.setDepth(8)
    enoch.setData('name', 'Enoch')
    enoch.setData('dialogue', 'I walked with God and was no more... for God took me.')

    // Passive wildlife - deer
    for (let i = 0; i < 8; i++) {
      const x = Phaser.Math.Between(800, 11200)
      const y = Phaser.Math.Between(800, 11200)
      const deer = this.wildlife.create(x, y, 'deer')
      deer.setDisplaySize(144, 120)
      deer.setDepth(6)
      deer.setData('type', 'deer')
      deer.setData('fleeRange', 480)
      deer.setData('patrolRadius', 320)
      deer.setData('originX', x)
      deer.setData('originY', y)
      deer.setData('state', 'graze')
      deer.setData('stateTimer', 0)
      deer.setData('speed', 160)
    }

    // Passive wildlife - birds
    for (let i = 0; i < 12; i++) {
      const x = Phaser.Math.Between(800, 11200)
      const y = Phaser.Math.Between(800, 11200)
      const bird = this.wildlife.create(x, y, 'bird')
      bird.setDisplaySize(84, 72)
      bird.setDepth(6)
      bird.setData('type', 'bird')
      bird.setData('fleeRange', 320)
      bird.setData('patrolRadius', 400)
      bird.setData('originX', x)
      bird.setData('originY', y)
      bird.setData('state', 'graze')
      bird.setData('stateTimer', 0)
      bird.setData('speed', 240)
    }

    // Passive wildlife - sheep
    for (let i = 0; i < 6; i++) {
      const x = Phaser.Math.Between(800, 11200)
      const y = Phaser.Math.Between(800, 11200)
      const sheep = this.wildlife.create(x, y, 'sheep')
      sheep.setDisplaySize(120, 96)
      sheep.setDepth(6)
      sheep.setData('type', 'sheep')
      sheep.setData('fleeRange', 440)
      sheep.setData('patrolRadius', 240)
      sheep.setData('originX', x)
      sheep.setData('originY', y)
      sheep.setData('state', 'graze')
      sheep.setData('stateTimer', 0)
      sheep.setData('speed', 140)
    }

    // Demons
    for (let i = 0; i < 12; i++) {
      const x = Phaser.Math.Between(800, 11200)
      const y = Phaser.Math.Between(800, 11200)
      const enemy = this.enemies.create(x, y, 'demon_mob')
      enemy.setDisplaySize(132, 150)
      enemy.setDepth(7)
      enemy.setData('health', 280)
      enemy.setData('maxHealth', 280)
      enemy.setData('damage', 80)
      enemy.setData('type', 'demon')
      enemy.setData('aggroRange', 640)
      enemy.setData('patrolRadius', 400)
      enemy.setData('originX', x)
      enemy.setData('originY', y)
      enemy.setData('state', 'patrol')
      enemy.setData('stateTimer', 0)
      enemy.setData('attackCooldown', 0)
      enemy.setData('speed', 220)
    }

    // Seth warriors
    for (let i = 0; i < 6; i++) {
      const x = Phaser.Math.Between(800, 11200)
      const y = Phaser.Math.Between(800, 11200)
      const enemy = this.enemies.create(x, y, 'seth_warrior')
      enemy.setDisplaySize(120, 164)
      enemy.setDepth(7)
      enemy.setData('health', 320)
      enemy.setData('maxHealth', 320)
      enemy.setData('damage', 80)
      enemy.setData('type', 'seth_warrior')
      enemy.setData('aggroRange', 640)
      enemy.setData('patrolRadius', 400)
      enemy.setData('originX', x)
      enemy.setData('originY', y)
      enemy.setData('state', 'patrol')
      enemy.setData('stateTimer', 0)
      enemy.setData('attackCooldown', 0)
      enemy.setData('speed', 180)
    }
  }

  createTheFlood() {
    console.log('GameScene: Creating The Flood...')

    // Water overlay
    const water = this.add.tileSprite(3000, 3000, 12000, 12000, 'water')
    water.setDepth(-1)
    water.setAlpha(0.5)

    // Ark pieces
    for (let i = 0; i < 10; i++) {
      const x = Phaser.Math.Between(400, 11600)
      const y = Phaser.Math.Between(400, 11600)
      const house = this.resources.create(x, y, 'house')
      house.setImmovable(true)
      house.setDisplaySize(160, 140)
      house.setDepth(5)
    }

    // Animals (represented as NPCs)
    const animal = this.npcs.create(1500, 1500, 'npc_woman')
    animal.setDisplaySize(96, 128)
    animal.setDepth(8)
    animal.setData('name', 'Animal Keeper')
    animal.setData('dialogue', 'Two by two, they entered the ark. The waters rise...')

    // Few surviving wildlife - scared deer
    for (let i = 0; i < 5; i++) {
      const x = Phaser.Math.Between(800, 11200)
      const y = Phaser.Math.Between(800, 11200)
      const deer = this.wildlife.create(x, y, 'deer')
      deer.setDisplaySize(144, 120)
      deer.setDepth(6)
      deer.setData('type', 'deer')
      deer.setData('fleeRange', 640)
      deer.setData('patrolRadius', 200)
      deer.setData('originX', x)
      deer.setData('originY', y)
      deer.setData('state', 'graze')
      deer.setData('stateTimer', 0)
      deer.setData('speed', 180)
    }

    // Flood demons
    for (let i = 0; i < 15; i++) {
      const x = Phaser.Math.Between(800, 11200)
      const y = Phaser.Math.Between(800, 11200)
      const enemy = this.enemies.create(x, y, 'flood_demon_mob')
      enemy.setDisplaySize(132, 150)
      enemy.setDepth(7)
      enemy.setData('health', 320)
      enemy.setData('maxHealth', 320)
      enemy.setData('damage', 120)
      enemy.setData('type', 'flood_demon')
      enemy.setData('aggroRange', 720)
      enemy.setData('patrolRadius', 480)
      enemy.setData('originX', x)
      enemy.setData('originY', y)
      enemy.setData('state', 'patrol')
      enemy.setData('stateTimer', 0)
      enemy.setData('attackCooldown', 0)
      enemy.setData('speed', 200)
    }

    // Serpents
    for (let i = 0; i < 8; i++) {
      const x = Phaser.Math.Between(800, 11200)
      const y = Phaser.Math.Between(800, 11200)
      const enemy = this.enemies.create(x, y, 'serpent_mob')
      enemy.setDisplaySize(144, 84)
      enemy.setDepth(7)
      enemy.setData('health', 240)
      enemy.setData('maxHealth', 240)
      enemy.setData('damage', 80)
      enemy.setData('type', 'serpent')
      enemy.setData('aggroRange', 600)
      enemy.setData('patrolRadius', 400)
      enemy.setData('originX', x)
      enemy.setData('originY', y)
      enemy.setData('state', 'patrol')
      enemy.setData('stateTimer', 0)
      enemy.setData('attackCooldown', 0)
      enemy.setData('speed', 240)
    }
  }

  update(time: number, delta: number) {
    // Player movement
    const speed = 200
    this.player.setVelocity(0)

    let isMoving = false
    let moveX = 0
    let moveY = 0

    if (this.cursors.left.isDown || this.keys.left.isDown) {
      moveX = -1
      isMoving = true
    } else if (this.cursors.right.isDown || this.keys.right.isDown) {
      moveX = 1
      isMoving = true
    }

    if (this.cursors.up.isDown || this.keys.up.isDown) {
      moveY = -1
      isMoving = true
    } else if (this.cursors.down.isDown || this.keys.down.isDown) {
      moveY = 1
      isMoving = true
    }

    this.player.setVelocityX(moveX * speed)
    this.player.setVelocityY(moveY * speed)
    
    // Track last direction for sword positioning and texture management
    const prevDirection = this.lastDirection
    if (moveY > 0) this.lastDirection = 'down'
    else if (moveY < 0) this.lastDirection = 'up'
    else if (moveX < 0) this.lastDirection = 'left'
    else if (moveX > 0) this.lastDirection = 'right'

    // Update sword position to follow player
    if (!this.isAttacking) {
      // Sword sheathed - position on player's back/hip
      const swordOffsetX = -8
      const swordOffsetY = 10
      this.sword.setPosition(
        this.player.x + swordOffsetX,
        this.player.y + swordOffsetY
      )
      this.sword.setTexture('sword_sheathed')
      this.sword.setDisplaySize(48, 160)
      this.sword.setRotation(0)
    }

    // Player texture management — only change when direction changes (prevents blinking)
    if (this.isAttacking) {
      this.player.setTexture('player_attack')
    } else if (isMoving) {
      // Only update texture when direction actually changes
      if (this.lastDirection !== prevDirection) {
        if (this.lastDirection === 'down') {
          this.player.setTexture('player_down')
          this.player.setFlipX(false)
        } else if (this.lastDirection === 'up') {
          this.player.setTexture('player_up')
          this.player.setFlipX(false)
        } else if (this.lastDirection === 'left') {
          this.player.setTexture('player_left')
          this.player.setFlipX(false)
        } else if (this.lastDirection === 'right') {
          this.player.setTexture('player_right')
          this.player.setFlipX(false)
        }
      }
    } else {
      // Idle — keep current direction texture, no blinking
      // Texture already set from last movement direction
      this.player.setFlipX(false)
    }

    // Attack
    if (Phaser.Input.Keyboard.JustDown(this.keys.attack)) {
      this.attack()
    }

    // Interact
    if (Phaser.Input.Keyboard.JustDown(this.keys.interact)) {
      this.interact()
    }

    // Change acts with number keys
    if (Phaser.Input.Keyboard.JustDown(this.keys.act1)) {
      this.changeAct('eden')
    } else if (Phaser.Input.Keyboard.JustDown(this.keys.act2)) {
      this.changeAct('hermon')
    } else if (Phaser.Input.Keyboard.JustDown(this.keys.act3)) {
      this.changeAct('nephilim')
    } else if (Phaser.Input.Keyboard.JustDown(this.keys.act4)) {
      this.changeAct('enoch')
    } else if (Phaser.Input.Keyboard.JustDown(this.keys.act5)) {
      this.changeAct('flood')
    }

    // Check for nearby NPCs
    this.checkNearbyNPCs()

    // Enemy AI
    this.updateEnemies(delta)

    // Wildlife AI
    this.updateWildlife(delta)

    // Depth sorting for player vs resources
    this.player.setDepth(10)

    // Update lighting
    this.updateLighting()

    // Update bloom and vignette
    this.bloomTime += delta
    this.updateBloomAndVignette()

    // Animate water
    this.children.list.forEach(child => {
      if (child instanceof Phaser.GameObjects.TileSprite && child.texture.key === 'water') {
        child.tilePositionX += 0.3
        child.tilePositionY += 0.1
      }
    })
  }

  attack() {
    if (this.isAttacking) return
    this.isAttacking = true

    // Direction based on movement or default right
    let dx = 0, dy = 0
    if (this.cursors.left.isDown || this.keys.left.isDown) dx = -1
    else if (this.cursors.right.isDown || this.keys.right.isDown) dx = 1
    else if (this.cursors.up.isDown || this.keys.up.isDown) dy = -1
    else if (this.cursors.down.isDown || this.keys.down.isDown) dy = 1
    else {
      // Use last direction
      if (this.lastDirection === 'left') dx = -1
      else if (this.lastDirection === 'right') dx = 1
      else if (this.lastDirection === 'up') dy = -1
      else dy = 1
    }

    // Phase 1: Unsheathe sword (0-200ms)
    this.sword.setTexture('sword_unsheathed')
    this.sword.setDisplaySize(64, 240)
    
    // Position sword based on direction
    let swordAngle = Math.atan2(dy, dx)
    let swordDist = 20
    
    // Unsheathe animation - sword comes up with sparkle trail
    const unsheatheEffect = this.add.sprite(
      this.player.x,
      this.player.y - 20,
      'sword_unsheath_effect'
    ).setDepth(25).setAlpha(0.8).setScale(0.5)
    
    this.tweens.add({
      targets: unsheatheEffect,
      alpha: 0,
      y: this.player.y - 50,
      duration: 300,
      onComplete: () => unsheatheEffect.destroy()
    })

    // Phase 2: Swing sword (200-500ms)
    this.time.delayedCall(200, () => {
      // Position sword in attack direction
      this.sword.setPosition(
        this.player.x + dx * 15,
        this.player.y + dy * 15
      )
      this.sword.setRotation(swordAngle + Math.PI / 4)
      this.sword.setDisplaySize(64, 280)

      // Position attack hitbox (Zone - always invisible)
      this.attackHitbox.setPosition(
        this.player.x + dx * 35,
        this.player.y + dy * 35
      )
      this.attackHitbox.setSize(60, 60)

      // Create slash effect
      const slash = this.add.sprite(
        this.player.x + dx * 35,
        this.player.y + dy * 35,
        'sword_slash'
      ).setDepth(20).setAlpha(0).setScale(0.3)

      slash.setRotation(swordAngle)

      this.tweens.add({
        targets: slash,
        alpha: 0.9,
        scale: 1.2,
        duration: 150,
        yoyo: true,
        onComplete: () => slash.destroy()
      })

      // Sword swing arc
      this.tweens.add({
        targets: this.sword,
        rotation: swordAngle - Math.PI / 4,
        duration: 300,
        ease: 'Power2'
      })

      // Bright flash on swing
      const flash = this.add.circle(
        this.player.x + dx * 30,
        this.player.y + dy * 30,
        20,
        0xffffff,
        0.6
      ).setDepth(22)

      this.tweens.add({
        targets: flash,
        alpha: 0,
        radius: 40,
        duration: 200,
        onComplete: () => flash.destroy()
      })
    })

    // Phase 3: Resheathe sword (500-700ms)
    this.time.delayedCall(500, () => {
      // Deactivate attack hitbox
      this.attackHitbox.setSize(0, 0)

      // Return sword to sheathed position
      this.tweens.add({
        targets: this.sword,
        rotation: 0,
        duration: 200,
        ease: 'Power2',
        onComplete: () => {
          this.sword.setTexture('sword_sheathed')
          this.sword.setDisplaySize(48, 160)
        }
      })
    })

    this.time.delayedCall(700, () => {
      this.isAttacking = false
    })
  }

  hitEnemy(projectile: any, enemy: any) {
    if (!projectile.active || !enemy.active) return
    projectile.destroy()

    const health = enemy.getData('health') || 100
    const maxHealth = enemy.getData('maxHealth') || 100
    enemy.setData('health', health - 50)

    // Flash effect
    enemy.setTint(0xff0000)
    this.time.delayedCall(100, () => {
      if (enemy.active) enemy.clearTint()
    })

    // Hit effect
    const hit = this.add.sprite(enemy.x, enemy.y, 'hit_effect').setDepth(25)
    this.tweens.add({
      targets: hit,
      alpha: 0,
      scale: 1.5,
      duration: 400,
      onComplete: () => hit.destroy()
    })

    // Health bar above enemy
    this.showEnemyHealthBar(enemy)

    // Check if enemy defeated
    if (enemy.getData('health') <= 0) {
      this.defeatEnemy(enemy)
    }
  }

  playerHitEnemy(player: any, enemy: any) {
    if (!enemy.active) return
    const damage = enemy.getData('damage') || 20
    this.playerHealth -= damage * 0.032
    this.player.setTint(0xff0000)
    this.time.delayedCall(100, () => {
      this.player.clearTint()
    })

    // Update UI
    const uiScene = this.scene.get('UIScene') as UIScene
    if (uiScene) {
      uiScene.updateHealth(this.playerHealth, this.playerMaxHealth)
    }

    // Check death
    if (this.playerHealth <= 0) {
      this.playerDeath()
    }
  }

  defeatEnemy(enemy: any) {
    // Death animation
    this.tweens.add({
      targets: enemy,
      alpha: 0,
      scale: 0.5,
      duration: 500,
      onComplete: () => {
        // EXP particles
        for (let i = 0; i < 5; i++) {
          const particle = this.add.sprite(enemy.x, enemy.y, 'exp_particle')
            .setDepth(30)
            .setTint(0xffd700)

          this.tweens.add({
            targets: particle,
            x: enemy.x + Phaser.Math.Between(-80, 80),
            y: enemy.y + Phaser.Math.Between(-80, 80),
            alpha: 0,
            scale: 0,
            duration: 600,
            onComplete: () => particle.destroy()
          })
        }

        enemy.destroy()
      }
    })
  }

  showEnemyHealthBar(enemy: any) {
    const health = enemy.getData('health')
    const maxHealth = enemy.getData('maxHealth')
    if (!health || !maxHealth) return

    const bar = this.add.graphics().setDepth(50)
    const barWidth = 40
    const barHeight = 4
    const x = enemy.x - barWidth / 2
    const y = enemy.y - (enemy.displayHeight / 2) - 10

    bar.fillStyle(0x333333)
    bar.fillRect(x, y, barWidth, barHeight)

    const percent = health / maxHealth
    bar.fillStyle(percent > 0.5 ? 0x228b22 : percent > 0.25 ? 0xffff00 : 0xff0000)
    bar.fillRect(x, y, barWidth * percent, barHeight)

    this.time.delayedCall(1500, () => bar.destroy())
  }

  interact() {
    this.npcs.getChildren().forEach((obj) => {
      const npc = obj as Phaser.Physics.Arcade.Sprite
      const distance = Phaser.Math.Distance.Between(
        this.player.x,
        this.player.y,
        npc.x,
        npc.y
      )

      if (distance < 80) {
        const name = npc.getData('name') || 'Unknown'
        const dialogue = npc.getData('dialogue') || '...'

        // Show dialogue in UI scene
        const uiScene = this.scene.get('UIScene') as UIScene
        if (uiScene) {
          uiScene.showDialogue(name, dialogue)
        }
      }
    })

    // Check for chests
    this.resources.getChildren().forEach((obj) => {
      const resource = obj as Phaser.Physics.Arcade.Sprite
      const distance = Phaser.Math.Distance.Between(
        this.player.x,
        this.player.y,
        resource.x,
        resource.y
      )

      if (distance < 80 && resource.getData('type') === 'chest') {
        const loot = resource.getData('loot') || 'Gold x10'
        const uiScene = this.scene.get('UIScene') as UIScene
        if (uiScene) {
          uiScene.showDialogue('Chest Opened', `Found: ${loot}!`)
        }

        // Open animation
        this.tweens.add({
          targets: resource,
          alpha: 0.5,
          duration: 300
        })

        resource.setData('type', 'opened_chest')
      }
    })
  }

  checkNearbyNPCs() {
    let nearby = false
    this.npcs.getChildren().forEach((obj) => {
      const npc = obj as Phaser.Physics.Arcade.Sprite
      const distance = Phaser.Math.Distance.Between(
        this.player.x,
        this.player.y,
        npc.x,
        npc.y
      )
      if (distance < 80) {
        nearby = true
      }
    })

    this.interactPrompt.setVisible(nearby)
    if (nearby) {
      this.interactPrompt.setText('Press E to interact')
    }
  }

  updateEnemies(delta: number) {
    this.enemies.getChildren().forEach((obj) => {
      const enemy = obj as Phaser.Physics.Arcade.Sprite
      if (!enemy.active) return

      const distance = Phaser.Math.Distance.Between(
        this.player.x,
        this.player.y,
        enemy.x,
        enemy.y
      )

      const aggroRange = enemy.getData('aggroRange') || 300
      const originX = enemy.getData('originX') || enemy.x
      const originY = enemy.getData('originY') || enemy.y
      const patrolRadius = enemy.getData('patrolRadius') || 200

      let state = enemy.getData('state') || 'patrol'
      let stateTimer = enemy.getData('stateTimer') || 0
      stateTimer += delta

      if (state === 'patrol') {
        // Wander around origin
        if (stateTimer > 2000 + Math.random() * 2000) {
          const angle = Math.random() * Math.PI * 2
          const dist = Math.random() * patrolRadius
          const targetX = originX + Math.cos(angle) * dist
          const targetY = originY + Math.sin(angle) * dist

          this.physics.moveTo(enemy, targetX, targetY, 60 + Math.random() * 40)
          enemy.setData('stateTimer', 0)
        }

        // Check if player is in aggro range
        if (distance < aggroRange) {
          state = 'chase'
          enemy.setData('state', 'chase')
          enemy.setData('stateTimer', 0)
        }
      } else if (state === 'chase') {
        // Chase player
        if (distance > 30) {
          this.physics.moveToObject(enemy, this.player, 160)
        }

        // Flip based on direction
        if (this.player.x < enemy.x) {
          enemy.setFlipX(true)
        } else {
          enemy.setFlipX(false)
        }

        // Lost player?
        if (distance > aggroRange * 4) {
          state = 'return'
          enemy.setData('state', 'return')
          enemy.setData('stateTimer', 0)
        }
      } else if (state === 'return') {
        // Return to origin
        const distToOrigin = Phaser.Math.Distance.Between(enemy.x, enemy.y, originX, originY)
        if (distToOrigin > 10) {
          this.physics.moveTo(enemy, originX, originY, 100)
        } else {
          state = 'patrol'
          enemy.setData('state', 'patrol')
          enemy.setData('stateTimer', 0)
          enemy.setVelocity(0)
        }
      }

      enemy.setData('stateTimer', stateTimer)
    })
  }

  updateWildlife(delta: number) {
    this.wildlife.getChildren().forEach((obj) => {
      const animal = obj as Phaser.Physics.Arcade.Sprite
      if (!animal.active) return

      const distance = Phaser.Math.Distance.Between(
        this.player.x,
        this.player.y,
        animal.x,
        animal.y
      )

      const fleeRange = animal.getData('fleeRange') || 240
      const originX = animal.getData('originX') || animal.x
      const originY = animal.getData('originY') || animal.y
      const patrolRadius = animal.getData('patrolRadius') || 160
      const speed = animal.getData('speed') || 80

      let state = animal.getData('state') || 'graze'
      let stateTimer = animal.getData('stateTimer') || 0
      stateTimer += delta

      if (state === 'graze') {
        // Wander slowly around origin
        if (stateTimer > 3000 + Math.random() * 3000) {
          const angle = Math.random() * Math.PI * 2
          const dist = Math.random() * patrolRadius
          const targetX = originX + Math.cos(angle) * dist
          const targetY = originY + Math.sin(angle) * dist

          this.physics.moveTo(animal, targetX, targetY, speed * 0.5)
          animal.setData('stateTimer', 0)
        }

        // Flip based on direction
        const body = animal.body as Phaser.Physics.Arcade.Body
        if (body && body.velocity.x < 0) {
          animal.setFlipX(true)
        } else if (body && body.velocity.x > 0) {
          animal.setFlipX(false)
        }

        // Flee if player is close
        if (distance < fleeRange) {
          state = 'flee'
          animal.setData('state', 'flee')
          animal.setData('stateTimer', 0)
        }
      } else if (state === 'flee') {
        // Run away from player
        const fleeAngle = Math.atan2(animal.y - this.player.y, animal.x - this.player.x)
        const fleeSpeed = speed * 2
        animal.setVelocity(
          Math.cos(fleeAngle) * fleeSpeed,
          Math.sin(fleeAngle) * fleeSpeed
        )

        // Flip based on direction
        if (this.player.x < animal.x) {
          animal.setFlipX(true)
        } else {
          animal.setFlipX(false)
        }

        // Safe distance or timeout
        if (distance > fleeRange * 4.5 || stateTimer > 6000) {
          state = 'graze'
          animal.setData('state', 'graze')
          animal.setData('stateTimer', 0)
          animal.setVelocity(0)
        }
      }

      animal.setData('stateTimer', stateTimer)
    })
  }

  meleeHitEnemy(hitbox: any, enemy: any) {
    if (!hitbox.active || !enemy.active || !this.isAttacking) return

    const health = enemy.getData('health') || 100
    const maxHealth = enemy.getData('maxHealth') || 100
    enemy.setData('health', health - 50)

    // Flash effect
    enemy.setTint(0xff0000)
    this.time.delayedCall(100, () => {
      if (enemy.active) enemy.clearTint()
    })

    // Hit effect
    const hit = this.add.sprite(enemy.x, enemy.y, 'hit_effect').setDepth(25)
    this.tweens.add({
      targets: hit,
      alpha: 0,
      scale: 1.5,
      duration: 400,
      onComplete: () => hit.destroy()
    })

    // Health bar above enemy
    this.showEnemyHealthBar(enemy)

    // Check if enemy defeated
    if (enemy.getData('health') <= 0) {
      this.defeatEnemy(enemy)
    }
  }

  updateLighting() {
    this.lightingOverlay.clear()

    // Base darkness
    this.lightingOverlay.fillStyle(0x000000, 0.15)
    this.lightingOverlay.fillRect(
      this.cameras.main.scrollX,
      this.cameras.main.scrollY,
      this.cameras.main.width,
      this.cameras.main.height
    )

    // Player light (circle of light around player)
    this.lightingOverlay.fillStyle(0xffffff, 0.85)
    this.lightingOverlay.fillCircle(this.player.x, this.player.y, 480)

    // Softer outer glow
    this.lightingOverlay.fillStyle(0xffffff, 0.3)
    this.lightingOverlay.fillCircle(this.player.x, this.player.y, 720)

    // Act-specific lighting
    if (this.currentAct === 'eden') {
      // Bright, sunny
      this.lightingOverlay.fillStyle(0xffffff, 0.1)
      this.lightingOverlay.fillCircle(this.player.x, this.player.y, 1000)
    } else if (this.currentAct === 'nephilim') {
      // Darker, ominous
      this.lightingOverlay.fillStyle(0xff4400, 0.05)
      this.lightingOverlay.fillCircle(this.player.x, this.player.y, 800)
    } else if (this.currentAct === 'flood') {
      // Blue-tinted
      this.lightingOverlay.fillStyle(0x4488ff, 0.05)
      this.lightingOverlay.fillCircle(this.player.x, this.player.y, 800)
    }
  }

  updateBloomAndVignette() {
    const camera = this.cameras.main
    const cx = camera.scrollX + camera.width / 2
    const cy = camera.scrollY + camera.height / 2

    // --- Bloom effect ---
    this.bloomGraphics.clear()

    // Subtle pulsing bloom intensity (HD-2D style glow)
    const pulse = 0.03 + Math.sin(this.bloomTime * 0.002) * 0.015

    // Player divine glow - warm golden aura
    this.bloomGraphics.fillStyle(0xffd700, pulse * 1.5)
    this.bloomGraphics.fillCircle(this.player.x, this.player.y, 400)
    this.bloomGraphics.fillStyle(0xffaa44, pulse)
    this.bloomGraphics.fillCircle(this.player.x, this.player.y, 600)

    // Act-specific bloom tints
    if (this.currentAct === 'eden') {
      // Warm golden bloom for paradise feel
      this.bloomGraphics.fillStyle(0xffee88, pulse * 0.5)
      this.bloomGraphics.fillCircle(this.player.x, this.player.y, 800)
    } else if (this.currentAct === 'nephilim') {
      // Fiery red-orange bloom for demonic atmosphere
      this.bloomGraphics.fillStyle(0xff4400, pulse * 0.8)
      this.bloomGraphics.fillCircle(this.player.x, this.player.y, 720)
    } else if (this.currentAct === 'flood') {
      // Cool blue bloom for water theme
      this.bloomGraphics.fillStyle(0x4488ff, pulse * 0.6)
      this.bloomGraphics.fillCircle(this.player.x, this.player.y, 720)
    } else if (this.currentAct === 'enoch') {
      // Divine golden sparkles
      this.bloomGraphics.fillStyle(0xffd700, pulse * 0.7)
      this.bloomGraphics.fillCircle(this.player.x, this.player.y, 640)
    }

    // Glow around enemies (subtle aura)
    this.enemies.getChildren().forEach((obj) => {
      const enemy = obj as Phaser.Physics.Arcade.Sprite
      if (!enemy.active) return

      const dist = Phaser.Math.Distance.Between(this.player.x, this.player.y, enemy.x, enemy.y)
      if (dist < 400) {
        const enemyAlpha = pulse * 0.4 * (1 - dist / 400)
        const enemyType = enemy.getData('type') || ''

        // Color based on enemy type
        let glowColor = 0xff4444 // default red glow
        if (enemyType === 'demon' || enemyType === 'flood_demon') glowColor = 0xff2200
        else if (enemyType === 'nephilim') glowColor = 0x8844ff
        else if (enemyType === 'watcher') glowColor = 0x4488ff
        else if (enemyType === 'serpent') glowColor = 0x44ff44

        this.bloomGraphics.fillStyle(glowColor, enemyAlpha)
        this.bloomGraphics.fillCircle(enemy.x, enemy.y, 200)
      }
    })

    // Weather particle glow (subtle ambient bloom)
    if (this.weatherParticles && this.weatherParticles.active) {
      this.bloomGraphics.fillStyle(0xffffff, pulse * 0.3)
      this.bloomGraphics.fillCircle(this.player.x, this.player.y, 1000)
    }

    // --- Vignette effect ---
    this.vignetteGraphics.clear()

    const vignetteWidth = camera.width
    const vignetteHeight = camera.height
    const vignetteCenterX = camera.scrollX + vignetteWidth / 2
    const vignetteCenterY = camera.scrollY + vignetteHeight / 2

    // Dark overlay across entire screen
    this.vignetteGraphics.fillStyle(0x000000, 0.25)
    this.vignetteGraphics.fillRect(
      camera.scrollX,
      camera.scrollY,
      vignetteWidth,
      vignetteHeight
    )

    // Cut out center with gradient (lighter in middle, darker at edges)
    const vignetteRadius = Math.max(vignetteWidth, vignetteHeight) * 0.6
    this.vignetteGraphics.fillStyle(0xffffff, 0.25)
    this.vignetteGraphics.fillCircle(vignetteCenterX, vignetteCenterY, vignetteRadius)

    const vignetteRadiusInner = Math.max(vignetteWidth, vignetteHeight) * 0.35
    this.vignetteGraphics.fillStyle(0xffffff, 0.15)
    this.vignetteGraphics.fillCircle(vignetteCenterX, vignetteCenterY, vignetteRadiusInner)

    // --- Color grading (warm overall tint) ---
    this.vignetteGraphics.fillStyle(0xffeedd, 0.04)
    this.vignetteGraphics.fillRect(
      camera.scrollX,
      camera.scrollY,
      vignetteWidth,
      vignetteHeight
    )
  }

  changeAct(act: string) {
    console.log('GameScene: Changing act to', act)
    this.scene.restart({ act })
  }

  playerDeath() {
    console.log('GameScene: Player died!')

    // Death effect
    this.tweens.add({
      targets: this.player,
      alpha: 0,
      duration: 1000,
      onComplete: () => {
        this.playerHealth = 200
        this.player.setPosition(1500, 1500)
        this.player.setAlpha(1)

        const uiScene = this.scene.get('UIScene') as UIScene
        if (uiScene) {
          uiScene.updateHealth(this.playerHealth, this.playerMaxHealth)
          uiScene.showDialogue('Death', 'You have fallen. Rise again!')
        }
      }
    })
  }
}

// UIScene for health bar and dialogue
export class UIScene extends Phaser.Scene {
  private healthBar!: Phaser.GameObjects.Graphics
  private dialogueBox!: Phaser.GameObjects.Container
  private dialogueText!: Phaser.GameObjects.Text
  private dialogueName!: Phaser.GameObjects.Text

  constructor() {
    super({ key: 'UIScene' })
  }

  create() {
    const width = this.cameras.main.width
    const height = this.cameras.main.height

    // Health bar — positioned relative to camera size for proper 1080p/4K scaling
    this.healthBar = this.add.graphics().setDepth(1000)
    this.updateHealthBar(100, 100)

    // Dialogue box (hidden by default) — responsive positioning
    const dialogueY = height - Math.min(300, height * 0.16)
    const dialogueW = Math.min(1200, width * 0.32)
    const dialogueH = Math.min(240, height * 0.12)
    const dialogueX = (width - dialogueW) / 2
    this.dialogueBox = this.add.container(dialogueX, dialogueY).setDepth(2002)
    this.dialogueBox.setVisible(false)

    const bg = this.add.rectangle(dialogueW / 2, dialogueH / 2, dialogueW, dialogueH, 0x000000, 0.85)
    bg.setStrokeStyle(3, 0xffffff)

    const nameFontSize = Math.min(36, height * 0.019)
    const textFontSize = Math.min(28, height * 0.015)

    this.dialogueName = this.add.text(20, 15, '', {
      font: `bold ${nameFontSize}px Arial`,
      color: '#ffd700'
    })

    this.dialogueText = this.add.text(20, 55, '', {
      font: `${textFontSize}px Arial`,
      color: '#ffffff',
      wordWrap: { width: dialogueW - 40 }
    })

    this.dialogueBox.add([bg, this.dialogueName, this.dialogueText])

    console.log('UIScene: Created successfully')
  }

  updateHealth(current: number, max: number) {
    this.updateHealthBar(current, max)
  }

  updateHealthBar(current: number, max: number) {
    this.healthBar.clear()

    // Responsive health bar sizing based on camera dimensions
    const width = this.cameras.main.width
    const height = this.cameras.main.height
    const barW = Math.min(1200, width * 0.32)
    const barH = Math.min(48, height * 0.025)
    const barX = Math.min(40, width * 0.01)
    const barY = Math.min(40, height * 0.02)
    const borderW = Math.min(4, height * 0.002)

    // Background
    this.healthBar.fillStyle(0x333333)
    this.healthBar.fillRect(barX, barY, barW, barH)

    // Health
    const percent = Math.max(0, current / max)
    this.healthBar.fillStyle(percent > 0.5 ? 0x228b22 : percent > 0.25 ? 0xffff00 : 0xff0000)
    this.healthBar.fillRect(barX, barY, barW * percent, barH)

    // Border
    this.healthBar.lineStyle(borderW, 0xffffff)
    this.healthBar.strokeRect(barX, barY, barW, barH)
  }

  showDialogue(name: string, text: string) {
    this.dialogueName.setText(name)
    this.dialogueText.setText(text)
    this.dialogueBox.setVisible(true)

    // Hide after 4 seconds
    this.time.delayedCall(4000, () => {
      this.dialogueBox.setVisible(false)
    })
  }
}
