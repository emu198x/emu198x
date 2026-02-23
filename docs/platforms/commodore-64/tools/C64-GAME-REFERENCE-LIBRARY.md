# C64 Game Reference Library

A comprehensive reference of commercial C64 games organized by technical features for lesson writing.

**Purpose:** Provide diverse, specific examples when teaching programming concepts. Avoid repeating the same 5-6 games.

**Usage:** When writing lessons, search this document for relevant technical features and choose games that best illustrate the concept.

---

## Index by Technical Feature

- [Sprite Animation](#sprite-animation)
- [Sprite Priority & Layering](#sprite-priority--layering)
- [Sprite Multiplexing](#sprite-multiplexing)
- [Scrolling Techniques](#scrolling-techniques)
- [Raster Effects](#raster-effects)
- [Sound & Music](#sound--music)
- [Collision Detection](#collision-detection)
- [Character Graphics](#character-graphics)
- [State Machines](#state-machines)
- [Data-Driven Design](#data-driven-design)

---

## Sprite Animation

### Simple Walk Cycles (2-4 frames)
- **Boulder Dash** (1984, First Star) - Rockford's iconic 2-frame walk
- **Lode Runner** (1983, Broderbund) - Minimal but effective animation
- **Manic Miner** (1983, Bug-Byte) - Miner Willy's simple walk cycle
- **Jet Set Willy** (1984, Software Projects) - Extended Willy animation
- **Monty Mole** (1984, Gremlin) - 3-frame walk with direction changes
- **Creatures** (1990, Apex) - Large sprite with smooth animation

### Complex Multi-State Animation
- **Impossible Mission** (1984, Epyx) - Running, jumping, falling, searching states
- **Kung Fu Master** (1985, Activision) - Punch, kick, walk, crouch states
- **International Karate** (1986, System 3) - 20+ martial arts moves
- **International Karate +** (1987, System 3) - Even more complex fighting moves
- **Barbarian** (1987, Palace) - Sword combat with multiple attack animations
- **The Last Ninja** (1987, System 3) - Complex combat and movement states
- **Mayhem in Monsterland** (1993, Apex) - Late-era animation showcase

### Directional Animation (4 or 8 directions)
- **Commando** (1985, Elite) - 8-direction soldier animation
- **Gauntlet** (1986, U.S. Gold) - 4-direction character animation for 4 players
- **Ikari Warriors** (1987, Elite) - 8-direction military animation
- **The Last Ninja 2** (1988, System 3) - Isometric 8-way movement
- **Paradroid** (1985, Hewson) - Simple but effective robot rotation

### Animated Enemies
- **Bubble Bobble** (1987, Firebird) - Multiple enemy types with distinct animations
- **Ghosts 'n Goblins** (1986, Elite) - Varied monster animations
- **R-Type** (1988, Electric Dreams) - Organic enemy animation patterns
- **Turrican** (1990, Rainbow Arts) - Large animated bosses
- **Katakis** (1988, Rainbow Arts) - Mechanical enemy animation

### Sprite Rotation/Transformation
- **Cybernoid** (1987, Hewson) - Ship rotation effects
- **Zynaps** (1987, Hewson) - Rotating enemy formations
- **Armalyte** (1988, Thalamus) - Weapon rotation effects
- **Delta** (1987, Thalamus) - Rotating delta-wing ship

---

## Sprite Priority & Layering

### Walking Behind Scenery
- **The Last Ninja** (1987, System 3) - Character passes behind bamboo, walls
- **Maniac Mansion** (1987, LucasArts) - Characters behind furniture
- **Raid over Moscow** (1984, Access) - Planes behind buildings
- **Boulder Dash** (1984, First Star) - Rockford behind rocks/walls
- **Rick Dangerous** (1989, Firebird) - Behind temple pillars and structures
- **Wonderland** (1988, Virgin) - Complex foreground/background layers

### Platform Depth
- **The Great Giana Sisters** (1987, Rainbow Arts) - Behind/in front of platforms
- **Creatures** (1990, Apex) - Multi-layer platform depth
- **Rainbow Islands** (1990, Ocean) - Platform layering effects
- **Parasol Stars** (1992, Ocean) - Depth in platform scenes

### Isometric Depth
- **Head Over Heels** (1987, Ocean) - Complex isometric layering
- **Batman** (1986, Ocean) - Isometric film-set with depth
- **Movie** (1986, Imagine) - Isometric perspective with priority

### Vertical Depth (Buildings/Towers)
- **Nebulus** (1987, Hewson) - Rotating tower with depth layers
- **Elevator Action** (1985, Taito) - Building floors with elevator priority

---

## Sprite Multiplexing

### Many Enemies on Screen
- **Uridium** (1986, Hewson) - Andrew Braybrook's famous multiplexing
- **Paradroid** (1985, Hewson) - Multiple robots on screen
- **Wizball** (1987, Ocean) - Many bullets and enemies
- **SEUCK games** (1987+, Sensible Software) - Shoot'Em Up Construction Kit engine
- **Armalyte** (1988, Thalamus) - Bullet hell patterns
- **Katakis** (1988, Rainbow Arts) - R-Type-style enemy swarms

### Vertical Shooters (Lots of Bullets)
- **1942** (1986, Elite) - Many planes and bullets
- **Flying Shark** (1987, Firebird) - Dense bullet patterns
- **Xenon** (1988, Melbourne House) - Multiplexed projectiles

### Horizontal Shooters
- **R-Type** (1988, Electric Dreams) - Many enemies and projectiles
- **Slap Fight** (1987, Imagine) - Bullet patterns
- **Scramble** (1982, Stern) - Early multiplexing techniques

---

## Scrolling Techniques

### Smooth Horizontal Scrolling
- **Defender** (1983, Atari) - Bi-directional smooth scrolling
- **Dropzone** (1984, Arena) - Defender-style scrolling
- **Scramble** (1982, Stern) - Early smooth scrolling
- **Mayhem in Monsterland** (1993, Apex) - Perfect smooth scrolling
- **Creatures** (1990, Apex) - Smooth horizontal scrolling

### Smooth Vertical Scrolling
- **1942** (1986, Elite) - Vertical shooter scrolling
- **Flying Shark** (1987, Firebird) - Smooth vertical scrolling
- **Commando** (1985, Elite) - Vertical combat scrolling

### 4-Way Scrolling
- **Paradroid** (1985, Hewson) - Ship deck scrolling
- **Gauntlet** (1986, U.S. Gold) - Dungeon scrolling (coarse)
- **The Last Ninja** (1987, System 3) - Garden/level scrolling

### Parallax Scrolling
- **Parallax** (1986, Sensible Software) - Named after the technique!
- **X-Out** (1989, Rainbow Arts) - Multi-layer underwater parallax
- **Turrican** (1990, Rainbow Arts) - Complex parallax backgrounds
- **Katakis** (1988, Rainbow Arts) - Parallax space backgrounds

### Coarse Scrolling (Character-based)
- **Zork** series (1980-1982, Infocom) - Text-based room transitions
- **Bard's Tale** (1985, Electronic Arts) - Dungeon movement
- **Ultima** series (1980s, Origin) - Tile-based overworld

---

## Raster Effects

### Color Bar Effects
- **International Karate** (1986, System 3) - Sky gradient with raster bars
- **Thrust** (1986, Firebird) - Status area color separation
- **Parallax** (1986, Sensible Software) - Raster bar backgrounds

### Split Screen Effects
- **International Karate +** (1987, System 3) - Split-screen two-player
- **Archon** (1984, Electronic Arts) - Board and battle screen split

### Dynamic Borders
- **Boing!** (1986, Demo) - Bouncing ball leaving playfield
- **Mayhem in Monsterland** (1993, Apex) - Full-screen visuals

### Color Cycling
- **Wizball** (1987, Ocean) - Rainbow color effects
- **Bubble Bobble** (1987, Firebird) - Flashing/cycling colors

---

## Sound & Music

### Rob Hubbard Compositions
- **Monty on the Run** (1985, Gremlin) - Iconic intro music
- **International Karate** (1986, System 3) - Memorable theme
- **Commando** (1985, Elite) - Action game music
- **Thrust** (1986, Firebird) - Atmospheric soundtrack
- **Sanxion** (1986, Thalamus) - Complex multi-voice music
- **Delta** (1987, Thalamus) - Layered soundtrack

### Martin Galway Compositions
- **Parallax** (1986, Sensible Software) - Electronic soundtrack
- **Armalyte** (1988, Thalamus) - Action music
- **Times of Lore** (1988, Origin) - RPG soundtrack

### Ben Daglish Compositions
- **The Last Ninja** (1987, System 3) - Oriental-themed music
- **Trap** (1986, Alligata) - Puzzle game music
- **Deflektor** (1987, Gremlin) - Puzzle soundtrack

### In-Game Music During Play
- **The Last Ninja** (1987, System 3) - Music continues during gameplay
- **Parallax** (1986, Sensible Software) - Music + SFX together
- **Cybernoid** (1987, Hewson) - Music throughout gameplay

### Sound Effects Only
- **Impossible Mission** (1984, Epyx) - Speech and SFX, minimal music
- **Ghostbusters** (1984, Activision) - Speech synthesis + effects
- **Impossible Mission II** (1988, Epyx) - More speech synthesis

---

## Collision Detection

### Pixel-Perfect Collision
- **Boulder Dash** (1984, First Star) - Precise rock/diamond collision
- **Impossible Mission** (1984, Epyx) - Platform collision precision
- **Lode Runner** (1983, Broderbund) - Ladder/trap collision

### Bounding Box Collision
- **Commando** (1985, Elite) - Bullet collision detection
- **1942** (1986, Elite) - Simple projectile collision
- **Scramble** (1982, Stern) - Rocket/bomb collision

### Sprite-to-Background Collision
- **Paradroid** (1985, Hewson) - Robot-to-wall collision
- **Gauntlet** (1986, U.S. Gold) - Wall collision detection
- **The Last Ninja** (1987, System 3) - Scenery collision

### Multiple Collision Layers
- **R-Type** (1988, Electric Dreams) - Weapon, ship, power-up collision
- **Bubble Bobble** (1987, Firebird) - Bubble, enemy, platform collision

---

## Character Graphics

### PETSCII Art
- **Zork** series (1980-1982, Infocom) - Text-based adventures
- **Deadline** (1982, Infocom) - Detective text adventure
- **Early type-in games** (Compute! Gazette) - PETSCII graphics

### Redefined Character Sets
- **Paradroid** (1985, Hewson) - Custom character set for ship interiors
- **Bard's Tale** (1985, Electronic Arts) - Dungeon tile graphics
- **Ultima IV** (1985, Origin) - Overworld character graphics

### Mixed Character/Sprite Games
- **Boulder Dash** (1984, First Star) - Character backgrounds, sprite Rockford
- **Impossible Mission** (1984, Epyx) - Character platforms, sprite agent
- **Lode Runner** (1983, Broderbund) - Character maze, sprite runner

---

## State Machines

### Game State Management
- **Paradroid** (1985, Hewson) - Title → Transfer game → Ship exploration
- **International Karate** (1986, System 3) - Title → Fight → Results → High scores
- **The Last Ninja** (1987, System 3) - Title → Level load → Play → Death → Continue

### Menu Systems
- **Wasteland** (1988, Electronic Arts) - Complex RPG menus
- **Bard's Tale** (1985, Electronic Arts) - Party management menus
- **Pool of Radiance** (1988, SSI) - D&D menu systems

### Multi-Level Games
- **Manic Miner** (1983, Bug-Byte) - 20 distinct levels
- **Jet Set Willy** (1984, Software Projects) - 60+ interconnected rooms
- **The Great Giana Sisters** (1987, Rainbow Arts) - 32 levels
- **Mayhem in Monsterland** (1993, Apex) - Multiple worlds

---

## Data-Driven Design

### Level Data from DATA Statements
- **Boulder Dash** (1984, First Star) - Cave layouts in data
- **Sokoban** (1984, Thinking Rabbit) - Puzzle layouts
- **Laser Squad** (1988, Blade) - Mission data structures

### Disk-Loaded Levels
- **The Last Ninja** (1987, System 3) - Multi-load levels
- **Turrican** (1990, Rainbow Arts) - Large level data
- **Mayhem in Monsterland** (1993, Apex) - Streamed level data

### Enemy Behavior Tables
- **Wizball** (1987, Ocean) - Enemy formation patterns
- **Uridium** (1986, Hewson) - Attack wave patterns
- **Bubble Bobble** (1987, Firebird) - Monster behavior patterns

---

## Additional Categories

### Text Adventures & Interactive Fiction
- **The Hobbit** (1982, Melbourne House) - Early parser adventure
- **Zork I-III** (1980-1982, Infocom) - Classic IF trilogy
- **Hitchhiker's Guide** (1984, Infocom) - Comedy IF
- **Leather Goddesses of Phobos** (1986, Infocom) - Advanced parser

### RPG Systems
- **Bard's Tale** (1985, Electronic Arts) - Party-based combat
- **Ultima IV** (1985, Origin) - Virtue system
- **Pool of Radiance** (1988, SSI) - D&D rules implementation
- **Wasteland** (1988, Electronic Arts) - Post-apocalyptic RPG

### Puzzle Games
- **Tetris** (1987, Mirrorsoft) - Block rotation and line clearing
- **Sokoban** (1984, Thinking Rabbit) - Box-pushing puzzles
- **Boulderdash** (1984, First Star) - Physics-based puzzles
- **Deflektor** (1987, Gremlin) - Laser-reflection puzzles

### Sports Games
- **International Soccer** (1983, Commodore) - Early sports game
- **World Games** (1986, Epyx) - Multiple event types
- **Leaderboard Golf** (1986, Access) - Golf simulation
- **Summer Games** (1984, Epyx) - Olympic events

### Racing Games
- **Pole Position** (1983, Atari) - Pseudo-3D racing
- **Pitstop II** (1984, Epyx) - Split-screen racing
- **Turbo Out Run** (1989, U.S. Gold) - Sprite-scaling racing

---

## Quick Selection Guide

**Need simple animation?** → Boulder Dash, Lode Runner, Manic Miner
**Need complex animation?** → Impossible Mission, Kung Fu Master, International Karate
**Need layering examples?** → Last Ninja, Maniac Mansion, Rick Dangerous
**Need scrolling?** → Defender, Scramble, Mayhem in Monsterland
**Need multiplexing?** → Uridium, Paradroid, Armalyte
**Need music examples?** → Rob Hubbard's catalogue (Monty, Commando, Delta)
**Need state machines?** → Paradroid, International Karate, any multi-level game

---

## Notes for Lesson Writers

1. **Vary your examples** - Don't use Last Ninja/Turrican/Paradroid in every lesson
2. **Date awareness** - Early games (1983-1985) have different constraints than late games (1989-1993)
3. **Developer diversity** - UK vs US teams had different approaches
4. **Genre matters** - Shooters vs adventures vs puzzles show different techniques
5. **Programmer legends** - Andrew Braybrook, Jeff Minter, Archer MacLean, Martin Walker
6. **Honest limitations** - Not everything ran well in BASIC, acknowledge assembly superiority

---

**Last Updated:** 2025-01-22
**Version:** 1.0 - Initial compilation
