//! Sparks where the pointer meets the window, and a starburst on a click.
//!
//! Two effects, because two different things are being marked. A spark is
//! local: it says the pointer is here, over a few frames, and is gone. A
//! starburst is a confirmation: it fires on a button and says that button did
//! something, so it is bigger, coloured, and lasts longer than a spark.
//!
//! The two are kept in one particle engine rather than two, because they differ
//! in how a particle is drawn and how long it lives and not in how it moves.
//!
//! Nothing here uses a random number generator. Every spread comes from the
//! golden angle and the fractional parts of a multiplier, which is what makes
//! the effect look the same on every run: a burst that scattered differently
//! each time would make the window feel like it was flickering.

use eframe::egui;

/// One spark.
struct Spark {
    position: egui::Pos2,
    velocity: egui::Vec2,
    /// Length of the streak, in points.
    length: f32,
    width: f32,
    colour: egui::Color32,
    /// Seconds this spark has left. Sparks fade rather than stop, so the tail
    /// thins out instead of vanishing.
    age: f32,
    life: f32,
}

/// Every spark in flight.
#[derive(Default)]
pub struct Sparks {
    sparks: Vec<Spark>,
    /// Positions where a spark was left recently, so a fast drag leaves a
    /// trail rather than a dotted line.
    trail: Vec<(egui::Pos2, f32)>,
}

impl Sparks {
    const SPARK_LIFE: f32 = 0.28;
    const TRAIL_SPACING: f32 = 9.0;

    /// Leave a spark at `position`, and along the way there if the pointer
    /// moved far enough since the last one.
    ///
    /// Called every frame with the pointer position, not on a click. A trail
    /// that only appeared on a click would be a dot, and the point of this is
    /// the movement.
    pub fn at_pointer(&mut self, position: egui::Pos2, moved: bool, hovering: bool) {
        // Nothing is emitted outside the window, and nothing while the button is
        // down: a drag is not a hover, and laying sparks through one would draw
        // over the thing being dragged.
        if !hovering {
            self.trail.clear();
            return;
        }
        // A still pointer lays nothing. Without this the effect never stops: the
        // trail ages out after a fraction of a second, so a pointer sitting
        // still over a control looks like a fresh arrival every frame and the
        // window repaints forever without the user having touched anything.
        //
        // So movement is the only trigger. The spacing is a second condition on
        // top of that, not a replacement: it is what keeps a fast drag from
        // laying one spark per frame, which would be a solid line rather than a
        // trail.
        if !moved {
            return;
        }
        let should_lay = match self.trail.last() {
            Some((last, _)) => position.distance(*last) > Self::TRAIL_SPACING,
            None => true,
        };
        if !should_lay {
            return;
        }
        let n = self.sparks.len() as f32;
        // Two sparks per visit, at opposing sides of a small ring, so the effect
        // reads as chips off metal rather than as a single dot repeated.
        for side in 0..2 {
            let angle = n * 2.399_963 + side as f32 * std::f32::consts::PI;
            let speed = 90.0 + 150.0 * frac(n * 0.618_034);
            self.sparks.push(Spark {
                position,
                velocity: egui::vec2(angle.cos() * speed, angle.sin() * speed - 40.0),
                length: 4.0 + 5.0 * frac(n * 0.569_840),
                width: 1.6,
                colour: crate::theme::ACCENT.gamma_multiply(0.7),
                age: 0.0,
                life: Self::SPARK_LIFE,
            });
        }
        self.trail.push((position, 0.0));
        if self.trail.len() > 24 {
            self.trail.remove(0);
        }
    }

    /// A starburst at `position`, in the accent colour.
    ///
    /// This is the confirmation that a control was pressed. It is bigger and
    /// slower than a spark, and it lives long enough to be read as an event
    /// rather than as a flash.
    pub fn starburst(&mut self, position: egui::Pos2) {
        const COUNT: usize = 14;
        for index in 0..COUNT {
            let n = index as f32;
            // Evenly spread, so the first frame is a star and not a clump.
            let angle = n * 2.399_963;
            let speed = 260.0 + 180.0 * frac(n * 0.618_034);
            self.sparks.push(Spark {
                position,
                velocity: egui::vec2(angle.cos() * speed, angle.sin() * speed),
                length: 9.0 + 7.0 * frac(n * 0.754_877),
                width: 2.2,
                colour: crate::theme::ACCENT,
                age: 0.0,
                // Longer than a spark, so the two are not the same effect at two
                // sizes.
                life: Self::SPARK_LIFE * 1.9,
            });
        }
    }

    /// Forget the pointer trail, because the pointer has left the window.
    ///
    /// The sparks already in flight keep going: a piece that was struck does not
    /// stop because the pointer moved away, and stopping them would make the
    /// effect cut off at the window edge.
    pub fn clear_pointer(&mut self) {
        self.trail.clear();
    }

    /// Whether anything is still alive, and so whether the window needs to
    /// repaint because of this.
    pub fn is_active(&self) -> bool {
        !self.sparks.is_empty() || !self.trail.is_empty()
    }

    /// Advance and draw over the whole window.
    pub fn draw(&mut self, ui: &mut egui::Ui, seconds: f32) {
        for entry in &mut self.trail {
            entry.1 -= seconds;
        }
        self.trail.retain(|(_, age)| *age > 0.0);

        if self.sparks.is_empty() {
            return;
        }
        let painter = ui.ctx().layer_painter(egui::LayerId::new(
            egui::Order::Tooltip,
            egui::Id::new("sparks"),
        ));

        for spark in &mut self.sparks {
            spark.age += seconds;
            // Gravity pulls them down and a little drag keeps them from
            // travelling in a straight line to the edge.
            spark.velocity.y += 700.0 * seconds;
            spark.velocity -= spark.velocity * 1.4 * seconds;
            spark.position += spark.velocity * seconds;
        }

        for spark in &self.sparks {
            let t = (spark.age / spark.life).clamp(0.0, 1.0);
            // A streak from where the spark is back along the way it came, so the
            // length of that streak grows as the spark slows. A fixed length
            // would make every spark the same mark, and a struck chip is
            // long where it is fast and short where it has slowed.
            let speed = spark.velocity.length();
            let reach = spark.length * (0.35 + 0.65 * (speed / 260.0).min(1.0));
            let tail = spark.position - spark.velocity * (reach / speed.max(1.0));
            let fade = 1.0 - t;
            painter.add(egui::Shape::line_segment(
                [tail, spark.position],
                egui::Stroke::new(spark.width * fade, spark.colour.gamma_multiply(fade)),
            ));
        }

        self.sparks.retain(|spark| spark.age < spark.life);
    }
}

/// The fractional part of a positive value, used to spread values out.
fn frac(value: f32) -> f32 {
    value - value.floor()
}
