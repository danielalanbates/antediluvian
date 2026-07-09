import { useEffect, useRef } from 'react'
import Phaser from 'phaser'
import { BootScene } from './game/scenes/BootScene'
import { MainMenuScene } from './game/scenes/MainMenuScene'
import { GameScene, UIScene } from './game/scenes/GameScene'

// 4K-native game resolution
const GAME_WIDTH = 3840
const GAME_HEIGHT = 2160

function App() {
  const containerRef = useRef<HTMLDivElement>(null)
  const gameRef = useRef<Phaser.Game | null>(null)

  useEffect(() => {
    if (gameRef.current || !containerRef.current) return

    const config: Phaser.Types.Core.GameConfig = {
      type: Phaser.WEBGL,
      width: GAME_WIDTH,
      height: GAME_HEIGHT,
      parent: containerRef.current,
      backgroundColor: '#1a1a2e',
      scene: [BootScene, MainMenuScene, GameScene, UIScene],
      physics: {
        default: 'arcade',
        arcade: {
          gravity: { x: 0, y: 0 },
          debug: false
        }
      },
      scale: {
        mode: Phaser.Scale.FIT,
        autoCenter: Phaser.Scale.CENTER_BOTH
      },
      render: {
        pixelArt: true,
        antialias: false,
        roundPixels: false
      }
    }

    console.log('Creating Phaser 4K game...')
    gameRef.current = new Phaser.Game(config)

    return () => {
      if (gameRef.current) {
        gameRef.current.destroy(true)
        gameRef.current = null
      }
    }
  }, [])

  return (
    <div
      ref={containerRef}
      style={{
        width: '100vw',
        height: '100vh',
        position: 'fixed',
        top: 0,
        left: 0,
        background: '#1a1a2e'
      }}
    />
  )
}

export default App
