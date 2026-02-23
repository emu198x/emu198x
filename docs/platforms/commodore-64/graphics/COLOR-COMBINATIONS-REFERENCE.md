# Color Combinations Reference

**For: Commodore 64 Programming**
**Updated:** 2025-01-18
**Source:** Commodore 64 Programmer's Reference Guide, Chapter 3

---

## Overview

Color television sets have limitations in displaying certain color combinations side-by-side on the same scan line. Some combinations produce blurred or distorted images due to how NTSC/PAL color encoding works. This reference helps you choose color combinations that work well together.

**Key principle:** Colors with similar luminance (brightness) or chroma (color frequency) can interfere with each other, causing blur and color bleeding.

---

## Color Compatibility Chart

The chart below shows which foreground/background color combinations work well together:

| Symbol | Quality | Description |
|--------|---------|-------------|
| ● | Excellent | Sharp, clear display with good contrast |
| ○ | Fair | Acceptable, minor artifacts possible |
| X | Poor | Blurry, color bleeding, avoid if possible |

### Screen Color (Background) vs Character Color (Foreground)

```
                        CHARACTER COLOR (Foreground)
         0  1  2  3  4  5  6  7  8  9 10 11 12 13 14 15
      ┌──────────────────────────────────────────────────
    0 │ X  ●  X  ●  ●  ●  X  ●  ●  X  ●  ●  ●  ●  ●  ●
    1 │ ●  X  ●  X  ●  ●  ●  X  ●  ●  ●  ●  ●  X  ●  ●
    2 │ X  ●  X  X  ●  X  X  ●  ●  X  ●  X  X  X  X  ●
    3 │ ●  X  X  X  X  ●  ●  X  X  X  X  ●  X  X  ●  X
S   4 │ ●  ●  X  X  X  X  X  X  X  X  X  X  X  X  X  ●
C   5 │ ●  ●  X  ●  X  X  X  X  X  X  X  ●  X  ●  X  ●
R   6 │ ●  ●  X  ●  X  X  X  X  X  X  X  X  X  ●  ●  ●
E   7 │ ●  X  ●  X  X  X  ●  X  ●  ●  ●  ●  ●  X  X  X
E   8 │ ●  ●  ●  X  X  X  X  ●  X  ●  X  X  X  X  X  ●
N   9 │ X  ●  X  X  X  X  X  ●  ●  X  ●  X  X  X  X  ●
   10 │ ●  ●  ●  X  X  X  X  ●  X  ●  X  X  X  X  X  ●
C  11 │ ●  ●  X  ●  X  X  X  ●  X  X  X  X  ●  ●  ●  ●
O  12 │ ●  ●  ●  X  X  X  ●  X  X  ●  X  ●  X  X  X  ●
L  13 │ ●  X  X  X  X  ●  ●  X  X  X  X  ●  X  X  X  X
O  14 │ ●  ●  X  ●  X  X  ●  X  X  X  X  ●  X  X  X  ●
R  15 │ ●  ●  ●  X  ●  ●  ●  X  X  ●  ●  ●  ●  X  ●  X
```

### Color Names

| Code | Color | Code | Color |
|------|-------|------|-------|
| 0 | Black | 8 | Orange |
| 1 | White | 9 | Brown |
| 2 | Red | 10 | Light Red |
| 3 | Cyan | 11 | Gray 1 (Dark Gray) |
| 4 | Purple | 12 | Gray 2 (Medium Gray) |
| 5 | Green | 13 | Light Green |
| 6 | Blue | 14 | Light Blue |
| 7 | Yellow | 15 | Gray 3 (Light Gray) |

---

## Excellent Color Combinations

These combinations provide sharp, clear text with good contrast:

### High Contrast Combinations

**Black background (0):**
- White (1) - Classic, highest contrast
- Cyan (3) - Bright, computer-like
- Purple (4) - Distinct, good readability
- Green (5) - Easy on eyes
- Yellow (7) - High visibility
- Orange (8) - Warm, readable
- Light Red (10) - Good contrast
- Light Green (13) - Soft, readable
- Light Blue (14) - Pleasant
- Gray 3 (15) - Subtle but clear

**White background (1):**
- Black (0) - Reverse video, highest contrast
- Red (2) - Bold, attention-getting
- Purple (4) - Royal appearance
- Green (5) - Natural
- Blue (6) - Professional
- Orange (8) - Warm
- Brown (9) - Earthy
- Gray 1 (11) - Subtle
- Gray 2 (12) - Balanced
- Light Green (13) - Soft
- Light Blue (14) - Cool
- Gray 3 (15) - Light contrast

**Blue background (6):**
- White (1) - Clean, professional
- Yellow (7) - Warning/caution look
- Light Blue (14) - Monochromatic
- Gray 3 (15) - Subtle depth

---

## Fair Color Combinations

These combinations are acceptable but may show minor artifacts:

**Use with caution:**
- Medium contrast combinations
- Colors with similar saturation
- May show slight color bleed on some TVs

**Examples:**
- Brown (9) on Black (0)
- Dark Gray (11) on Black (0)
- Many gray-on-color combinations

---

## Poor Color Combinations (Avoid)

These combinations produce blur, color bleeding, or loss of definition:

### Common Problems

**Same color on same color:**
- Black on Black (0/0) - Invisible
- Red on Red (2/2) - No contrast
- Any color on itself - Never works

**Similar hue and brightness:**
- Red (2) on Black (0) - Bleeds badly
- Cyan (3) on White (1) - Washes out
- Purple (4) on itself or similar colors
- Many blue/cyan combinations

**High chroma interference:**
- Red (2) on Cyan (3) - Severe bleeding
- Purple (4) on Green (5) - Color distortion
- Bright colors on bright colors - Artifacts

---

## Practical Guidelines

### For Maximum Readability

**Best practices:**
1. **High contrast:** Use colors far apart in brightness (black/white, dark/light)
2. **Cool vs warm:** Mix cool colors (blues, cyans) with warm (yellows, reds)
3. **Test on real hardware:** Emulators may not show the same artifacts as real TVs

**Recommended combinations:**
- Black background + White text
- Black background + Cyan text (computer terminal feel)
- Black background + Yellow text (high visibility)
- White background + Blue text (professional documents)
- Blue background + White text (modern interface)
- Blue background + Yellow text (warning messages)

### For Game Graphics

**Character games:**
- Black (0) or Blue (6) backgrounds work well
- Use White (1), Cyan (3), Yellow (7) for important text
- Reserve Red (2) for danger/warnings (use sparingly)

**Sprites:**
- Sprites can use more color variety than text
- Movement reduces perception of color bleeding
- Test combinations on target display

### For Applications

**Productivity software:**
- Black on White or White on Black
- Blue on White for headers
- Avoid red/green for color-blind users

**Games:**
- High contrast for HUD/status
- Can be more creative with in-game graphics
- Movement masks some color issues

---

## Platform Differences

### NTSC vs PAL

**NTSC (North America):**
- More prone to color bleeding
- Chroma dot crawl more visible
- Follow chart strictly

**PAL (Europe):**
- Generally better color stability
- Still show artifacts with poor combinations
- Chart still applicable

### Monitor vs TV

**RGB Monitor:**
- Shows all combinations clearly
- No NTSC/PAL color artifacts
- Chart less critical (all combinations work)

**Composite TV:**
- Shows all artifacts mentioned
- Chart is essential guide
- Test on actual target display

---

## Special Considerations

### Multicolor Mode

**Additional colors available:**
- Uses bit-pairs instead of individual bits
- Can mix 4 colors per character
- Same combination rules apply within each character

**Recommendations:**
- Choose 4 colors that all work well together
- Test all pair combinations from the chart
- Avoid mixing "poor" combinations

### Extended Background Mode

**4 background colors:**
- Each must work with chosen foreground color
- Select 4 backgrounds that are all "excellent" or "fair" with foreground
- Maintain readability across all sections

---

## Quick Reference

### Safe Universal Combinations

These work well in any mode, any platform:

| Background | Foreground | Use Case |
|------------|-----------|----------|
| Black (0) | White (1) | Text, menus, general UI |
| Black (0) | Cyan (3) | Computer terminals, data |
| Black (0) | Yellow (7) | Warnings, highlights |
| White (1) | Black (0) | Documents, reverse video |
| White (1) | Blue (6) | Professional text |
| Blue (6) | White (1) | Modern interfaces |
| Blue (6) | Yellow (7) | Caution displays |

### Colors to Avoid Together

| Background | Foreground | Problem |
|------------|-----------|---------|
| Black (0) | Red (2) | Severe bleeding |
| Red (2) | Cyan (3) | Color interference |
| Purple (4) | Most colors | Limited compatibility |
| Same | Same | Invisible (obviously) |

---

## Testing Your Combinations

### Test Procedure

1. Display sample text using chosen combination
2. View on target hardware (TV or monitor)
3. Check for:
   - Blur around character edges
   - Color bleeding into adjacent areas
   - Loss of sharpness
   - Readability at normal viewing distance

### Adjustment Tips

**If combination is poor:**
- Increase contrast (darker bg / lighter fg or vice versa)
- Choose colors farther apart on chart
- Use white, black, or blue as base colors

**If combination is fair:**
- Test on multiple displays
- Consider alternative for critical text
- May be acceptable for non-critical graphics

---

## References

- **Official Documentation:** Commodore 64 Programmer's Reference Guide, Chapter 3
- **Related:** See SCREEN-COLOR-MEMORY-REFERENCE.md for color control
- **Related:** See VIC-II-GRAPHICS-MODES-REFERENCE.md for graphics modes

---

**Document Version:** 1.0
**Last Updated:** 2025-01-18
**Based on:** Official Commodore Technical Specifications and NTSC/PAL Testing
