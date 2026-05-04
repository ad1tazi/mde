# Heading 1 — Cyan (dark) / Blue (light)

## Heading 2 — Blue (dark) / Dark Green (light)

### Heading 3 — Magenta (both)

#### Heading 4 — Gray (dark) / DarkGray (light)

##### Heading 5 — same tier as h4

###### Heading 6 — same tier as h4

---

## Lists

- First bullet
- Second bullet
  - Nested bullet
- Third bullet

* Star marker
+ Plus marker

1. Ordered one
2. Ordered two

- [x] Done task
- [ ] Open task

---

## Mixed content under each heading

### A subsection with **bold** and *italic*

Some paragraph text. Headings above should be readable. Bullets below should be visible:

- visible bullet one
- visible bullet two

#### Smaller subheading

Lorem ipsum dolor sit amet. The h4 marker should be a softer gray that still
shows up against your terminal background.

---

## How to test

1. Open this file: `cargo run -- test_theme.md`
2. Switch your terminal between light and dark mode
3. Re-launch `mde` after each switch (theme is detected once at startup)
4. Headings, bullets, and task markers should remain readable in both modes
