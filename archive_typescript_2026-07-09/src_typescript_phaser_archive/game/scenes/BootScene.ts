import Phaser from 'phaser'

export class BootScene extends Phaser.Scene {
  constructor() {
    super({ key: 'BootScene' })
  }

  preload() {
    const width = this.scale.width
    const height = this.scale.height

    const loadingText = this.add.text(width / 2, height / 2 - 50, 'Loading Game...', {
      font: 'bold 48px Arial',
      color: '#ffffff'
    }).setOrigin(0.5)

    const progressBar = this.add.graphics()
    const progressBox = this.add.graphics()
    progressBox.fillStyle(0x222222, 0.8)
    progressBox.fillRect(width / 2 - 200, height / 2 + 20, 400, 40)

    this.load.on('progress', (value: number) => {
      progressBar.clear()
      progressBar.fillStyle(0x4a90d9, 1)
      progressBar.fillRect(width / 2 - 190, height / 2 + 30, 380 * value, 20)
    })

    this.load.on('complete', () => {
      progressBar.destroy()
      progressBox.destroy()
      loadingText.destroy()
    })

    // Load pre-rendered 4K-quality pixel art PNGs
    const assets = 'assets'
    this.load.image('player', `${assets}/player_down.png`)
    this.load.image('player_down', `${assets}/player_down.png`)
    this.load.image('player_up', `${assets}/player_down.png`)
    this.load.image('player_left', `${assets}/player_down.png`)
    this.load.image('player_right', `${assets}/player_down.png`)

    // Wildlife
    this.load.image('deer', `${assets}/deer.png`)
    this.load.image('rabbit', `${assets}/rabbit.png`)
    this.load.image('bird', `${assets}/bird.png`)
    this.load.image('fox', `${assets}/fox.png`)
    this.load.image('goat', `${assets}/goat.png`)
    this.load.image('sheep', `${assets}/sheep.png`)

    // Enemies
    this.load.image('serpent_mob', `${assets}/serpent_mob.png`)
    this.load.image('demon_mob', `${assets}/demon_mob.png`)
    this.load.image('watcher_mob', `${assets}/watcher_mob.png`)
    this.load.image('seth_warrior', `${assets}/seth_warrior.png`)
    this.load.image('nephilim_giant', `${assets}/nephilim_giant.png`)

    // Environment
    this.load.image('tree', `${assets}/tree.png`)
    this.load.image('rock', `${assets}/rock.png`)
    this.load.image('herb', `${assets}/herb.png`)
    this.load.image('grass', `${assets}/grass.png`)
    this.load.image('dirt', `${assets}/dirt.png`)
    this.load.image('exp_particle', `${assets}/exp_particle.png`)
    this.load.image('chest', `${assets}/chest.png`)
    this.load.image('house', `${assets}/house.png`)

    // NPCs
    this.load.image('npc_elder', `${assets}/npc_elder.png`)
    this.load.image('npc_woman', `${assets}/npc_woman.png`)

    // Weapons
    this.load.image('sword_sheathed', `${assets}/sword_sheathed.png`)
    this.load.image('sword_unsheathed', `${assets}/sword_unsheathed.png`)
  }

  create() {
    console.log('BootScene: All 4K pixel art textures loaded')

    // Generate directional player sprites from base texture
    this.generateDirectionalSprite('player_down', 'player_up', (ctx: CanvasRenderingContext2D, w: number, h: number) => {
      // Up = flip vertically (show back of character)
      ctx.translate(0, h)
      ctx.scale(1, -1)
    })

    this.generateDirectionalSprite('player_down', 'player_left', (ctx: CanvasRenderingContext2D, w: number, h: number) => {
      // Left = flip horizontally
      ctx.translate(w, 0)
      ctx.scale(-1, 1)
    })

    this.generateDirectionalSprite('player_down', 'player_right', (ctx: CanvasRenderingContext2D, w: number, h: number) => {
      // Right = original (no transform needed, base is facing right-ish)
    })

    this.scene.start('MainMenuScene')
  }

  private generateDirectionalSprite(sourceKey: string, targetKey: string, transform: (ctx: CanvasRenderingContext2D, w: number, h: number) => void) {
    const sourceTexture = this.textures.get(sourceKey)
    const sourceImage = sourceTexture.getSourceImage() as HTMLImageElement
    const w = sourceImage.width
    const h = sourceImage.height

    const canvas = document.createElement('canvas')
    canvas.width = w
    canvas.height = h
    const ctx = canvas.getContext('2d')!

    ctx.save()
    transform(ctx, w, h)
    ctx.drawImage(sourceImage, 0, 0)
    ctx.restore()

    // Apply slight color tint to differentiate directions
    const imageData = ctx.getImageData(0, 0, w, h)
    const data = imageData.data
    for (let i = 0; i < data.length; i += 4) {
      if (targetKey === 'player_up') {
        // Slightly darker for back view (shadow effect)
        data[i] = Math.floor(data[i] * 0.85)
        data[i + 1] = Math.floor(data[i + 1] * 0.85)
        data[i + 2] = Math.floor(data[i + 2] * 0.9)
      } else if (targetKey === 'player_left') {
        // Slight blue tint for left profile
        data[i + 2] = Math.min(255, data[i + 2] + 10)
      } else if (targetKey === 'player_right') {
        // Slight warm tint for right profile
        data[i] = Math.min(255, data[i] + 8)
        data[i + 1] = Math.min(255, data[i + 1] + 5)
      }
    }
    ctx.putImageData(imageData, 0, 0)

    // Add to Phaser texture cache
    this.textures.addCanvas(targetKey, canvas as any)
  }
}
