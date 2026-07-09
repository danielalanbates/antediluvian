import Phaser from 'phaser'

export class MainMenuScene extends Phaser.Scene {
  private particles!: Phaser.GameObjects.Particles.ParticleEmitter | null

  constructor() {
    super({ key: 'MainMenuScene' })
  }

  create() {
    const width = this.cameras.main.width
    const height = this.cameras.main.height

    // Animated background gradient (simulated with layered rectangles)
    const bg = this.add.rectangle(0, 0, width, height, 0x0a0a2e)
    bg.setOrigin(0)

    // Stars background
    for (let i = 0; i < 80; i++) {
      const x = Phaser.Math.Between(0, width)
      const y = Phaser.Math.Between(0, height)
      const size = Phaser.Math.FloatBetween(0.5, 2.5)
      const star = this.add.circle(x, y, size, 0xffffff, Phaser.Math.FloatBetween(0.3, 0.8))
      this.tweens.add({
        targets: star,
        alpha: star.alpha > 0.5 ? 0.2 : 0.8,
        duration: 1000 + Math.random() * 2000,
        yoyo: true,
        repeat: -1,
        ease: 'Sine.easeInOut'
      })
    }

    // Divine light from top
    const lightGrad = this.add.graphics()
    lightGrad.fillStyle(0x4488ff, 0.08)
    lightGrad.fillTriangle(width / 2 - 200, -50, width / 2 + 200, -50, width / 2, height / 2)
    lightGrad.setDepth(0)

    // Particle system for divine sparkles
    this.particles = this.add.particles(0, 0, 'exp_particle', {
      x: { min: width * 0.2, max: width * 0.8 },
      y: -10,
      speedY: { min: 20, max: 60 },
      speedX: { min: -20, max: 20 },
      lifespan: 4000,
      frequency: 150,
      scale: { start: 0.6, end: 0 },
      alpha: { start: 0.5, end: 0 },
      tint: [0xffd700, 0x88ccff, 0xffffff],
      emitting: true
    }).setDepth(1)

    // Title with glow effect
    const titleGlow = this.add.text(width / 2 + 2, height / 4 + 2, 'THE BOOK OF ENOCH', {
      font: 'bold 52px Arial',
      color: '#4488ff'
    }).setOrigin(0.5).setAlpha(0.3).setDepth(2)

    const title = this.add.text(width / 2, height / 4, 'THE BOOK OF ENOCH', {
      font: 'bold 52px Arial',
      color: '#ffd700',
      stroke: '#8b4513',
      strokeThickness: 6
    }).setOrigin(0.5).setDepth(3)

    // Subtitle
    const subtitle = this.add.text(width / 2, height / 4 + 60, 'A Biblical MMORPG Adventure', {
      font: 'italic 24px Arial',
      color: '#c0c0c0',
      stroke: '#000000',
      strokeThickness: 3
    }).setOrigin(0.5).setDepth(3)

    // Decorative line
    const line = this.add.graphics().setDepth(3)
    line.lineStyle(2, 0xffd700, 0.6)
    line.beginPath()
    line.moveTo(width / 2 - 200, height / 4 + 90)
    line.lineTo(width / 2 + 200, height / 4 + 90)
    line.strokePath()

    // Floating sword/shield icon (decorative)
    const shield = this.add.graphics().setDepth(2)
    shield.fillStyle(0x6b7b8d, 0.6)
    shield.fillEllipse(width / 2, height / 4 + 120, 30, 36)
    shield.fillStyle(0x8b9bad, 0.4)
    shield.fillEllipse(width / 2, height / 4 + 118, 22, 28)
    shield.fillStyle(0xffd700, 0.5)
    shield.fillCircle(width / 2, height / 4 + 120, 6)

    // Animate shield floating
    this.tweens.add({
      targets: shield,
      y: height / 4 + 115,
      duration: 2000,
      yoyo: true,
      repeat: -1,
      ease: 'Sine.easeInOut'
    })

    // Menu buttons
    const buttonStartY = height / 2 + 80
    const buttonSpacing = 65

    // New Game button
    const newGameBtn = this.createButton(width / 2, buttonStartY, '⚔️  New Game', () => {
      this.startGame()
    }, 0x2a6e2a)

    // Continue button
    const continueBtn = this.createButton(width / 2, buttonStartY + buttonSpacing, '📜  Continue', () => {
      this.startGame()
    }, 0x4a6fa5)
    continueBtn.setAlpha(0.5)

    // Acts selector
    this.add.text(width / 2, buttonStartY + buttonSpacing * 2.2, '— Select Your Journey —', {
      font: 'italic 20px Arial',
      color: '#aaaaaa',
      stroke: '#000000',
      strokeThickness: 2
    }).setOrigin(0.5).setDepth(3)

    // Act buttons
    const acts = [
      { name: '🌿 Eden', key: 'eden', color: 0x228b22 },
      { name: '⛰️ Mt. Hermon', key: 'hermon', color: 0x8b4513 },
      { name: '👹 Nephilim', key: 'nephilim', color: 0xd93030 },
      { name: '✨ Enoch', key: 'enoch', color: 0x4169e1 },
      { name: '🌊 The Flood', key: 'flood', color: 0x1a5276 }
    ]

    acts.forEach((act, index) => {
      const x = width / 2 - 320 + index * 160
      const actBtn = this.createButton(x, buttonStartY + buttonSpacing * 3.2, act.name, () => {
        this.startGame(act.key)
      }, act.color, 120, 40)

      // Hover effect
      actBtn.on('pointerover', () => {
        this.tweens.add({
          targets: actBtn,
          scale: 1.15,
          duration: 150,
          ease: 'Back.easeOut'
        })
      })
      actBtn.on('pointerout', () => {
        this.tweens.add({
          targets: actBtn,
          scale: 1,
          duration: 150,
          ease: 'Back.easeOut'
        })
      })
    })

    // Controls info
    this.add.text(20, height - 60, 'Controls: WASD/Arrows: Move  |  Space: Attack  |  E: Interact  |  1-5: Change Acts', {
      font: '13px Arial',
      color: '#666666'
    }).setDepth(3)

    // Version
    this.add.text(width - 20, height - 20, 'v1.0.0', {
      font: '11px Arial',
      color: '#444444'
    }).setOrigin(1, 1).setDepth(3)

    console.log('MainMenuScene: Created successfully')
  }

  createButton(x: number, y: number, text: string, callback: () => void, color: number = 0x4a90d9, width: number = 140, height: number = 42) {
    const button = this.add.container(x, y).setDepth(5)

    // Shadow
    const shadow = this.add.rectangle(2, 3, width, height, 0x000000, 0.3)
    shadow.setOrigin(0.5)

    // Background
    const bg = this.add.rectangle(0, 0, width, height, color)
    bg.setOrigin(0.5)
    bg.setStrokeStyle(2, 0xffffff, 0.6)
    bg.setInteractive({ useHandCursor: true })

    // Highlight (top edge)
    const highlight = this.add.rectangle(0, -height / 2 + 2, width - 4, 3, 0xffffff, 0.2)
    highlight.setOrigin(0.5)

    const label = this.add.text(0, 0, text, {
      font: 'bold 16px Arial',
      color: '#ffffff',
      stroke: '#000000',
      strokeThickness: 2
    }).setOrigin(0.5)

    button.add([shadow, bg, highlight, label])
    button.setSize(width, height)

    bg.on('pointerdown', callback)

    return button
  }

  startGame(actKey?: string) {
    console.log('MainMenuScene: Starting game with act:', actKey || 'eden')

    // Transition effect
    this.cameras.main.fadeOut(500, 0, 0, 0)

    this.time.delayedCall(500, () => {
      this.scene.start('GameScene', { act: actKey || 'eden' })
      this.scene.launch('UIScene')
      this.cameras.main.resetFX()
    })
  }
}
