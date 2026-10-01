//! A short burst of coloured paper, drawn over the window when a colour is set.
//!
//! Setting a colour is the one action in this tool that is immediately visible
//! on the mouse itself, so it is worth marking in the window as well. The burst
//! takes the colour that was chosen, which ties what the window shows to what the
//! mouse does.

use eframe::egui;

/// One piece of paper.
struct Particle {
    /// Where it is now, in points relative to the top left of the window.
    position: egui::Pos2,
    velocity: egui::Vec2,
    angle: f32,
    spin: f32,
    size: f32,
    colour: egui::Color32,
}

/// The burst currently in flight, if any.
#[derive(Default)]
pub struct Confetti {
    particles: Vec<Particle>,
    /// Seconds left before the burst is cleared.
    remaining: f32,
}

impl Confetti {
    /// Seconds the burst lasts. Long enough to be seen, short enough not to be
    /// in the way while the next colour is chosen.
    const LIFETIME: f32 = 1.8;
    const GRAVITY: f32 = 900.0;
    /// How fast the pieces leave the centre.
    const SPEED: f32 = 460.0;

    /// Start a burst in `colour` from `origin`.
    pub fn burst(&mut self, colour: [u8; 3], origin: egui::Pos2) {
        let base = egui::ecolor::Hsva::from(egui::Color32::from_rgb(
            colour[0], colour[1], colour[2],
        ));
        self.remaining = Self::LIFETIME;
        const COUNT: usize = 90;

        self.particles = (0..COUNT)
            .map(|index| {
                let n = index as f32;
                // Golden angle spacing rather than a random angle: the first frame
                // is then an even ring, and only the speeds differ. A fully random
                // burst leaves visible gaps straight away.
                let angle = n * 2.399_963;
                // Three coprime multipliers, the usual trick for spreading
                // consecutive indices over a range without a random number
                // generator, so the burst looks the same every time. The speed
                // spread is wide on purpose: with a narrow one the ring stays a
                // ring and travels outwards as a visible spiral.
                let speed = Self::SPEED * (0.28 + 0.72 * frac(n * 0.618_034));
                // Each piece takes a hue next to the chosen one, spread across
                // the burst, so the paper reads as one colour family rather than
                // as unrelated confetti.
                let hue = (base.h + n / COUNT as f32 * 0.18 - 0.09).rem_euclid(1.0);
                Particle {
                    position: origin,
                    // Lifted as well as pushed out, so the pieces go up first and
                    // come back down, which is what makes it read as an
                    // explosion rather than a spray.
                    velocity: egui::vec2(
                        angle.cos() * speed,
                        angle.sin() * speed - Self::SPEED * 0.55,
                    ),
                    angle,
                    spin: (0.5 - frac(n * 0.754_877)) * 16.0,
                    // Real confetti is a rectangle, and a square does not show the
                    // rotation: the shape has to be uneven to read as a piece of
                    // paper rather than a dot.
                    size: 5.0 + frac(n * 0.569_840) * 6.0,
                    colour: egui::Color32::from(egui::ecolor::Hsva {
                        h: hue,
                        s: (base.s + 0.25).min(1.0),
                        v: (base.v + 0.2).min(1.0),
                        a: 1.0,
                    }),
                }
            })
            .collect();
    }

    /// Whether anything is still falling.
    pub fn is_active(&self) -> bool {
        self.remaining > 0.0
    }

    /// Advance the burst by `seconds` and draw it over the whole window.
    ///
    /// `ui` is only used for the context and the rectangle to cover; the pieces
    /// go onto their own layer so they are not clipped by the panel they are
    /// drawn over.
    pub fn draw(&mut self, ui: &mut egui::Ui, seconds: f32) {
        if self.particles.is_empty() {
            return;
        }
        // The window is not cleared between frames, so the pieces are painted on
        // an overlay layer that starts empty each time. Painting them onto the
        // panel itself would leave a trail of ghosts behind them.
        let painter = ui.ctx().layer_painter(egui::LayerId::new(
            egui::Order::Tooltip,
            egui::Id::new("confetti"),
        ));

        for particle in &mut self.particles {
            particle.velocity.y += Self::GRAVITY * seconds;
            // A little sideways drag, so the pieces flutter instead of falling
            // like stones.
            particle.velocity.x -= particle.velocity.x * 0.6 * seconds;
            particle.position += particle.velocity * seconds;
            particle.angle += particle.spin * seconds;

            // A rectangle, not a square and not a dot: the rotation is only
            // readable if the shape is not symmetric under it, and a square at
            // this size looks like a pixel.
            let half_width = particle.size * 0.35;
            let half_height = particle.size * 0.7;
            let (sin, cos) = particle.angle.sin_cos();
            let local = [(-1.0, -1.0), (1.0, -1.0), (1.0, 1.0), (-1.0, 1.0)];
            let corners: Vec<egui::Pos2> = local
                .iter()
                .map(|(x, y)| {
                    let dx = x * half_width;
                    let dy = y * half_height;
                    particle.position + egui::vec2(dx * cos - dy * sin, dx * sin + dy * cos)
                })
                .collect();
            painter.add(egui::Shape::convex_polygon(
                corners,
                particle.colour,
                egui::Stroke::NONE,
            ));
        }

        self.remaining -= seconds;
        if self.remaining <= 0.0 {
            self.particles.clear();
        }
    }
}

/// The fractional part of a positive value, used to spread values out.
fn frac(value: f32) -> f32 {
    value - value.floor()
}
